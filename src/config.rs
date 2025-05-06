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
        let ch_home = home.join(".ch-vms");
        
        let asset_dir = env::var("CH_ASSET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| ch_home.join("assets"));
            
        let vm_root = env::var("CH_VM_DIR")
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
        
        let cpus = env::var("CH_CPUS")
            .map(|v| v.parse().unwrap_or(2))
            .unwrap_or(2);
            
        let mem = env::var("CH_MEM").unwrap_or_else(|_| "1024M".to_string());
        let disk_size = env::var("CH_DISK_SIZE").unwrap_or_else(|_| "10G".to_string());
        
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
