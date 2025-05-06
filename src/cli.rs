use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about = "Cloud-Hypervisor VM Manager", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new VM
    Create {
        /// Name of the VM
        name: String,
        
        /// Path to user-data file (optional)
        user_data: Option<String>,
    },
    
    /// List all VMs
    List,
    
    /// Get VM details
    Get {
        /// Name of the VM
        name: String,
    },
    
    /// Start a VM
    Start {
        /// Name of the VM
        name: String,
    },
    
    /// Stop a VM
    Stop {
        /// Name of the VM
        name: String,
    },
    
    /// Delete a VM
    Delete {
        /// Name of the VM
        name: String,
    },
    
    /// Forward host port to guest port
    PortForward {
        /// Name of the VM
        name: String,
        
        /// Host port
        host_port: u16,
        
        /// Guest port
        guest_port: u16,
    },
}
