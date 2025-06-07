use crate::error::{Error, Result};
use std::env;
use std::path::PathBuf;

pub struct Config {
    pub ch_home: PathBuf,
    pub asset_dir: PathBuf,
    pub vm_root: PathBuf,
    pub os_url: String,
    pub fw_url: String,
    pub ch_url: String,
    pub cr_url: String,
    pub base_raw: PathBuf,
    pub fw_bin: PathBuf,
    pub ch_bin: PathBuf,
    pub cr_bin: PathBuf,
    pub cpus: usize,
    pub mem: String,
    pub disk_size: String,
}

impl Config {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| Error::HomeDirNotFound)?;
        let ch_home = home.join(".meda");
        
        let asset_dir = env::var("MEDA_ASSET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| ch_home.join("assets"));
            
        let vm_root = env::var("MEDA_VM_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| ch_home.join("vms"));
            
        let os_url = "https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img".to_string();
        let fw_url = "https://github.com/cloud-hypervisor/rust-hypervisor-firmware/releases/latest/download/hypervisor-fw".to_string();
        let ch_url = "https://github.com/cloud-hypervisor/cloud-hypervisor/releases/latest/download/cloud-hypervisor-static".to_string();
        let cr_url = "https://github.com/cloud-hypervisor/cloud-hypervisor/releases/latest/download/ch-remote-static".to_string();
        
        let base_raw = asset_dir.join("ubuntu-base.raw");
        let fw_bin = asset_dir.join("hypervisor-fw");
        let ch_bin = asset_dir.join("cloud-hypervisor");
        let cr_bin = asset_dir.join("ch-remote");
        
        let cpus = env::var("MEDA_CPUS")
            .map(|v| v.parse().unwrap_or(2))
            .unwrap_or(2);
            
        let mem = env::var("MEDA_MEM").unwrap_or_else(|_| "1024M".to_string());
        let disk_size = env::var("MEDA_DISK_SIZE").unwrap_or_else(|_| "10G".to_string());
        
        Ok(Self {
            ch_home,
            asset_dir,
            vm_root,
            os_url,
            fw_url,
            ch_url,
            cr_url,
            base_raw,
            fw_bin,
            ch_bin,
            cr_bin,
            cpus,
            mem,
            disk_size,
        })
    }
    
    pub fn vm_dir(&self, name: &str) -> PathBuf {
        self.vm_root.join(name)
    }
    
    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.ch_home)?;
        std::fs::create_dir_all(&self.asset_dir)?;
        std::fs::create_dir_all(&self.vm_root)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_config_new_with_defaults() {
        // Save existing env vars
        let saved_asset_dir = env::var("MEDA_ASSET_DIR").ok();
        let saved_vm_dir = env::var("MEDA_VM_DIR").ok();
        let saved_cpus = env::var("MEDA_CPUS").ok();
        let saved_mem = env::var("MEDA_MEM").ok();
        let saved_disk_size = env::var("MEDA_DISK_SIZE").ok();

        // Remove all env vars to test defaults
        env::remove_var("MEDA_ASSET_DIR");
        env::remove_var("MEDA_VM_DIR");
        env::remove_var("MEDA_CPUS");
        env::remove_var("MEDA_MEM");
        env::remove_var("MEDA_DISK_SIZE");

        let config = Config::new().unwrap();
        
        assert!(config.ch_home.ends_with(".meda"));
        assert!(config.asset_dir.ends_with("assets"));
        assert!(config.vm_root.ends_with("vms"));
        assert_eq!(config.cpus, 2);
        assert_eq!(config.mem, "1024M");
        assert_eq!(config.disk_size, "10G");

        // Restore env vars
        if let Some(val) = saved_asset_dir { env::set_var("MEDA_ASSET_DIR", val); }
        if let Some(val) = saved_vm_dir { env::set_var("MEDA_VM_DIR", val); }
        if let Some(val) = saved_cpus { env::set_var("MEDA_CPUS", val); }
        if let Some(val) = saved_mem { env::set_var("MEDA_MEM", val); }
        if let Some(val) = saved_disk_size { env::set_var("MEDA_DISK_SIZE", val); }
    }

    #[test]
    fn test_config_new_with_env_vars() {
        let temp_dir = TempDir::new().unwrap();
        let asset_dir = temp_dir.path().join("custom_assets");
        let vm_dir = temp_dir.path().join("custom_vms");

        env::set_var("MEDA_ASSET_DIR", asset_dir.to_str().unwrap());
        env::set_var("MEDA_VM_DIR", vm_dir.to_str().unwrap());
        env::set_var("MEDA_CPUS", "4");
        env::set_var("MEDA_MEM", "2048M");
        env::set_var("MEDA_DISK_SIZE", "20G");

        let config = Config::new().unwrap();
        
        assert_eq!(config.asset_dir, asset_dir);
        assert_eq!(config.vm_root, vm_dir);
        assert_eq!(config.cpus, 4);
        assert_eq!(config.mem, "2048M");
        assert_eq!(config.disk_size, "20G");

        env::remove_var("MEDA_ASSET_DIR");
        env::remove_var("MEDA_VM_DIR");
        env::remove_var("MEDA_CPUS");
        env::remove_var("MEDA_MEM");
        env::remove_var("MEDA_DISK_SIZE");
    }

    #[test]
    fn test_vm_dir() {
        env::remove_var("MEDA_VM_DIR");
        let config = Config::new().unwrap();
        let vm_dir = config.vm_dir("test-vm");
        assert!(vm_dir.ends_with("vms/test-vm"));
    }

    #[test]
    fn test_ensure_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let asset_dir = temp_dir.path().join("assets");
        let vm_dir = temp_dir.path().join("vms");

        env::set_var("MEDA_ASSET_DIR", asset_dir.to_str().unwrap());
        env::set_var("MEDA_VM_DIR", vm_dir.to_str().unwrap());

        let config = Config::new().unwrap();
        config.ensure_dirs().unwrap();
        
        assert!(config.ch_home.exists());
        assert!(config.asset_dir.exists());
        assert!(config.vm_root.exists());

        env::remove_var("MEDA_ASSET_DIR");
        env::remove_var("MEDA_VM_DIR");
    }
}
