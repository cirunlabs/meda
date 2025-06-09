mod cli;
mod config;
mod error;
mod image;
mod network;
mod util;
mod vm;

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
        Commands::Create {
            name,
            user_data,
            force,
            memory,
            cpus,
            disk,
        } => {
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
            let resources = vm::VmResources::from_config_with_overrides(
                &config,
                memory.as_deref(),
                cpus,
                disk.as_deref(),
            );
            vm::create(&config, &name, user_data.as_deref(), &resources, cli.json).await?;
        }
        Commands::List => {
            vm::list(&config, cli.json).await?;
        }
        Commands::Get { name } => {
            vm::get(&config, &name, cli.json).await?;
        }
        Commands::Ip { name } => {
            vm::ip(&config, &name, cli.json).await?;
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
        Commands::PortForward {
            name,
            host_port,
            guest_port,
        } => {
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
        Commands::Pull {
            image,
            registry,
            org,
        } => {
            image::pull(
                &config,
                &image,
                registry.as_deref(),
                org.as_deref(),
                cli.json,
            )
            .await?;
        }
        Commands::Push {
            name,
            image,
            registry,
            dry_run,
        } => {
            image::push(
                &config,
                &name,
                &image,
                registry.as_deref(),
                dry_run,
                cli.json,
            )
            .await?;
        }
        Commands::Images => {
            image::list(&config, cli.json).await?;
        }
        Commands::Rmi {
            image,
            registry,
            org,
            force,
        } => {
            image::remove(
                &config,
                &image,
                registry.as_deref(),
                org.as_deref(),
                force,
                cli.json,
            )
            .await?;
        }
        Commands::Prune { all, force } => {
            image::prune(&config, all, force, cli.json).await?;
        }
        Commands::CreateImage {
            name,
            tag,
            registry,
            org,
            from_vm,
        } => {
            let default_registry = registry.as_deref().unwrap_or("ghcr.io");
            let default_org = org.as_deref().unwrap_or("cirunlabs");

            if let Some(vm_name) = from_vm {
                image::create_from_vm(
                    &config,
                    &vm_name,
                    &name,
                    &tag,
                    default_registry,
                    default_org,
                    cli.json,
                )
                .await?;
            } else {
                image::create_base_image(
                    &config,
                    &name,
                    &tag,
                    default_registry,
                    default_org,
                    cli.json,
                )
                .await?;
            }
        }
        Commands::Run {
            image,
            name,
            registry,
            org,
            user_data,
            no_start,
            memory,
            cpus,
            disk,
        } => {
            let resources = vm::VmResources::from_config_with_overrides(
                &config,
                memory.as_deref(),
                cpus,
                disk.as_deref(),
            );
            let options = image::RunOptions {
                vm_name: name.as_deref(),
                registry: registry.as_deref(),
                org: org.as_deref(),
                user_data_path: user_data.as_deref(),
                no_start,
                resources,
            };
            image::run_from_image(&config, &image, options, cli.json).await?;
        }
    }

    Ok(())
}
