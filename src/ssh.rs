use crate::config::Config;
use crate::error::{Error, Result};
use log::info;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

pub struct SshKeyPair {
    pub private_key_path: PathBuf,
    pub public_key: String,
}

/// Ensures an ED25519 SSH keypair exists at ~/.meda/ssh/id_ed25519.
/// Generates one if not present. Returns the key paths and public key content.
pub fn ensure_ssh_keypair(config: &Config) -> Result<SshKeyPair> {
    let ssh_dir = config.ssh_dir();
    let private_key_path = ssh_dir.join("id_ed25519");
    let public_key_path = ssh_dir.join("id_ed25519.pub");

    if private_key_path.exists() && public_key_path.exists() {
        let public_key = fs::read_to_string(&public_key_path)?;
        info!(
            "Using existing SSH keypair at {}",
            private_key_path.display()
        );
        return Ok(SshKeyPair {
            private_key_path,
            public_key: public_key.trim().to_string(),
        });
    }

    // Create ssh directory with 0700 permissions
    fs::create_dir_all(&ssh_dir)?;
    fs::set_permissions(&ssh_dir, fs::Permissions::from_mode(0o700))?;

    info!("Generating SSH keypair at {}", private_key_path.display());

    let output = Command::new("ssh-keygen")
        .arg("-t")
        .arg("ed25519")
        .arg("-f")
        .arg(&private_key_path)
        .arg("-N")
        .arg("")
        .arg("-C")
        .arg("meda@localhost")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::CommandFailed(format!(
            "ssh-keygen failed: {}",
            stderr
        )));
    }

    // Set 0600 on private key
    fs::set_permissions(&private_key_path, fs::Permissions::from_mode(0o600))?;

    let public_key = fs::read_to_string(&public_key_path)?;
    info!("SSH keypair generated successfully");

    Ok(SshKeyPair {
        private_key_path,
        public_key: public_key.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn test_ensure_ssh_keypair_generates_keys() {
        let temp_dir = TempDir::new().unwrap();
        let asset_dir = temp_dir.path().join("assets");
        let vm_dir = temp_dir.path().join("vms");

        std::env::set_var("MEDA_ASSET_DIR", asset_dir.to_str().unwrap());
        std::env::set_var("MEDA_VM_DIR", vm_dir.to_str().unwrap());

        let mut config = Config::new().unwrap();
        config.ch_home = temp_dir.path().join(".meda");

        let keypair = ensure_ssh_keypair(&config).unwrap();

        assert!(keypair.private_key_path.exists());
        assert!(keypair.public_key.contains("ssh-ed25519"));
        assert!(keypair.public_key.contains("meda@localhost"));

        // Verify permissions
        let metadata = fs::metadata(&keypair.private_key_path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);

        let ssh_dir = config.ssh_dir();
        let dir_metadata = fs::metadata(&ssh_dir).unwrap();
        assert_eq!(dir_metadata.permissions().mode() & 0o777, 0o700);

        std::env::remove_var("MEDA_ASSET_DIR");
        std::env::remove_var("MEDA_VM_DIR");
    }

    #[test]
    #[serial]
    fn test_ensure_ssh_keypair_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let asset_dir = temp_dir.path().join("assets");
        let vm_dir = temp_dir.path().join("vms");

        std::env::set_var("MEDA_ASSET_DIR", asset_dir.to_str().unwrap());
        std::env::set_var("MEDA_VM_DIR", vm_dir.to_str().unwrap());

        let mut config = Config::new().unwrap();
        config.ch_home = temp_dir.path().join(".meda");

        let keypair1 = ensure_ssh_keypair(&config).unwrap();
        let keypair2 = ensure_ssh_keypair(&config).unwrap();

        // Same public key both times
        assert_eq!(keypair1.public_key, keypair2.public_key);

        std::env::remove_var("MEDA_ASSET_DIR");
        std::env::remove_var("MEDA_VM_DIR");
    }
}
