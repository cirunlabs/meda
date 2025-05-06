mod cli;
mod config;
mod error;
mod network;
mod vm;
mod util;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use error::Result;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger with more verbose output
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();
    
    let cli = Cli::parse();
    let config = Config::new()?;
    
    info!("Meda - Cloud-Hypervisor VM Manager");
    info!("Working with VMs in: {}", config.vm_root.display());
    
    match cli.command {
        Commands::Create { name, user_data, force } => {
            if force {
                info!("Force flag set, removing existing VM if present");
                let vm_dir = config.vm_dir(&name);
                if vm_dir.exists() {
                    if vm::check_vm_running(&config, &name)? {
                        info!("Stopping existing VM: {}", name);
                        vm::stop(&config, &name).await?;
                    }
                    info!("Deleting existing VM: {}", name);
                    vm::delete(&config, &name).await?;
                }
            }
            vm::create(&config, &name, user_data.as_deref()).await?;
        }
        Commands::List => {
            vm::list(&config).await?;
        }
        Commands::Get { name } => {
            vm::get(&config, &name).await?;
        }
        Commands::Start { name } => {
            vm::start(&config, &name).await?;
        }
        Commands::Stop { name } => {
            vm::stop(&config, &name).await?;
        }
        Commands::Delete { name } => {
            vm::delete(&config, &name).await?;
        }
        Commands::PortForward { name, host_port, guest_port } => {
            network::port_forward(&config, &name, host_port, guest_port).await?;
        }
    }
    
    Ok(())
}
