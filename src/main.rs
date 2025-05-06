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
    env_logger::init();
    
    let cli = Cli::parse();
    let config = Config::new()?;
    
    info!("Meda - Cloud-Hypervisor VM Manager");
    
    match cli.command {
        Commands::Create { name, user_data } => {
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
