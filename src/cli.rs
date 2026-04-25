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

        /// Memory size (e.g., 1G, 2048M, 512M)
        #[arg(long)]
        memory: Option<String>,

        /// Number of CPUs
        #[arg(long)]
        cpus: Option<u8>,

        /// Disk size (e.g., 10G, 20G, 5120M)
        #[arg(long)]
        disk: Option<String>,

        /// VFIO device path for PCI passthrough (repeatable, e.g., /sys/bus/pci/devices/0000:01:00.0)
        #[arg(long)]
        device: Vec<String>,
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

    /// Run a VM from an image — classic cold-boot path (~27s). Use
    /// `meda run` without --cold for the auto-template fast path
    /// (~1.5s once the template is built).
    ///
    /// Flags:
    ///   --cold    force cold-boot even if a template is cached
    #[command(alias = "cold-run")]
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

        /// Memory size (e.g., 1G, 2048M, 512M)
        #[arg(long)]
        memory: Option<String>,

        /// Number of CPUs
        #[arg(long)]
        cpus: Option<u8>,

        /// Disk size (e.g., 10G, 20G, 5120M)
        #[arg(long)]
        disk: Option<String>,

        /// VFIO device path for PCI passthrough (repeatable, e.g., /sys/bus/pci/devices/0000:01:00.0)
        #[arg(long)]
        device: Vec<String>,

        /// Skip the auto-template fast path and cold-boot as before.
        #[arg(long)]
        cold: bool,

        /// After the VM is ready, exec into it with ssh. The VM
        /// keeps running after you exit the shell; clean it up
        /// with `meda delete <vm_name>`.
        #[arg(long)]
        ssh: bool,
    },

    /// Clean up orphaned TAP devices
    Cleanup {
        /// Show what would be cleaned up without actually doing it
        #[arg(long)]
        dry_run: bool,
    },

    /// Snapshot a running VM to its own dir (for fast restore later)
    Snapshot {
        /// Name of the VM
        name: String,
    },

    /// Restore a VM from its snapshot (~500ms vs ~27s cold boot)
    Restore {
        /// Name of the VM
        name: String,
    },

    /// List VMs that have a snapshot (i.e. are ready to fast-restore)
    Templates,

    /// Clone a snapshotted VM into a new one (fast-restore ready)
    Clone {
        /// Source VM (must have a snapshot)
        template: String,

        /// Name of the new VM
        new_name: String,
    },

    /// Start REST API server
    Serve {
        /// Port to bind to (default: 7777)
        #[arg(long, short, default_value = "7777")]
        port: u16,

        /// Host to bind to (default: 127.0.0.1)
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
}
