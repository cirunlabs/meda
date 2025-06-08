use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about = "Cloud-Hypervisor VM Manager", long_about = None)]
pub struct Cli {
    /// Output in JSON format
    #[arg(long, global = true)]
    pub json: bool,
    
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
        
        /// Force create (delete if exists)
        #[arg(short, long)]
        force: bool,
    },
    
    /// List all VMs
    List,
    
    /// Get VM details
    Get {
        /// Name of the VM
        name: String,
    },
    
    /// Get VM IP address
    Ip {
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
    
    /// Pull an image from a registry
    Pull {
        /// Image name with optional tag (e.g., ubuntu-noble:latest)
        image: String,
        
        /// Registry URL (default: ghcr.io)
        #[arg(long)]
        registry: Option<String>,
        
        /// Organization/namespace (default: trycua)
        #[arg(long)]
        org: Option<String>,
    },
    
    /// Push an image to a registry
    Push {
        /// Local image name
        name: String,
        
        /// Target image name with tag (e.g., my-registry/my-image:v1.0)
        image: String,
        
        /// Registry URL (default: ghcr.io)
        #[arg(long)]
        registry: Option<String>,
        
        /// Dry run - don't actually push
        #[arg(long)]
        dry_run: bool,
    },
    
    /// List cached images
    Images,
    
    /// Remove a specific image
    Rmi {
        /// Image name and tag (e.g., ubuntu:latest, ubuntu)
        image: String,
        
        /// Registry URL (default: ghcr.io)
        #[arg(long)]
        registry: Option<String>,
        
        /// Organization/namespace (default: cirunlabs)
        #[arg(long)]
        org: Option<String>,
        
        /// Force removal without confirmation
        #[arg(short, long)]
        force: bool,
    },
    
    /// Remove unused images
    Prune {
        /// Remove all images (not just unused ones)
        #[arg(long)]
        all: bool,
        
        /// Don't prompt for confirmation
        #[arg(short, long)]
        force: bool,
    },
    
    /// Create an image locally from base Ubuntu components
    CreateImage {
        /// Image name (e.g., ubuntu, my-custom-vm)
        name: String,
        
        /// Image tag (default: latest)
        #[arg(short, long, default_value = "latest")]
        tag: String,
        
        /// Registry URL (default: ghcr.io)
        #[arg(long)]
        registry: Option<String>,
        
        /// Organization/namespace (default: cirunlabs)
        #[arg(long)]
        org: Option<String>,
        
        /// Create from existing VM instead of base image
        #[arg(long)]
        from_vm: Option<String>,
    },
    
    /// Run a VM from an image
    Run {
        /// Image reference (e.g., ubuntu:latest, ghcr.io/cirunlabs/ubuntu:v1.0)
        image: String,
        
        /// VM name (optional, defaults to image name + timestamp)
        #[arg(short, long)]
        name: Option<String>,
        
        /// Registry URL (default: ghcr.io)
        #[arg(long)]
        registry: Option<String>,
        
        /// Organization/namespace (default: cirunlabs)
        #[arg(long)]
        org: Option<String>,
        
        /// Path to user-data file (optional)
        #[arg(long)]
        user_data: Option<String>,
        
        /// Don't start the VM, just create it
        #[arg(long)]
        no_start: bool,
    },
}
