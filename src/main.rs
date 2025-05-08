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
    // std::env::set_var("RUST_LOG", "info");
    env_logger::init();
    
    let cli = Cli::parse();
    let config = Config::new()?;
    
    info!("Meda - Cloud-Hypervisor VM Manager");
    info!("Working with VMs in: {}", config.vm_root.display());
    
    match cli.command {
        Commands::Create { name, user_data, force } => {
            if force {
                if !cli.json {
                    info!("Force flag set, removing existing VM if present");
                }
                let vm_dir = config.vm_dir(&name);
                if vm_dir.exists() {
                    if vm::check_vm_running(&config, &name)? {
                        if !cli.json {
                            info!("Stopping existing VM: {}", name);
                        }
                        vm::stop(&config, &name, cli.json).await?;
                    }
                    if !cli.json {
                        info!("Deleting existing VM: {}", name);
                    }
                    vm::delete(&config, &name, cli.json).await?;
                }
            }
            vm::create(&config, &name, user_data.as_deref(), cli.json).await?;
        }
        Commands::List => {
            vm::list(&config, cli.json).await?;
        }
        Commands::Get { name } => {
            vm::get(&config, &name, cli.json).await?;
        }
        Commands::Start { name } => {
            vm::start(&config, &name, cli.json).await?;
        }
        Commands::Stop { name } => {
            vm::stop(&config, &name, cli.json).await?;
        }
        Commands::Delete { name } => {
            vm::delete(&config, &name, cli.json).await?;
        }
        Commands::PortForward { name, host_port, guest_port } => {
            let result = network::port_forward(&config, &name, host_port, guest_port).await;
            if cli.json {
                if result.is_ok() {
                    let json_result = vm::VmResult {
                        success: true,
                        message: format!("Port forwarding set up: {} -> {}", host_port, guest_port),
                    };
                    println!("{}", serde_json::to_string_pretty(&json_result)?);
                } else if let Err(e) = result {
                    let json_result = vm::VmResult {
                        success: false,
                        message: format!("Error: {}", e),
                    };
                    println!("{}", serde_json::to_string_pretty(&json_result)?);
                }
            } else {
                result?;
            }
        }
    }
    
    Ok(())
}
