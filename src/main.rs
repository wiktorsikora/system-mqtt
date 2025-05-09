mod app;
mod cli;
mod config;
mod discovery;
mod home_assistant;
mod lm_sensors_impl;
mod mqtt;
mod password;
mod system_sensors;
mod nvidia_gpu;
mod utils;

use crate::cli::{Arguments, SubCommand};
use crate::config::load_config;
use anyhow::Result;
use log::Level;
use systemd_journal_logger::JournalLog;
use std::time::Duration;
use tokio::time;
use tokio::signal;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Arguments = argh::from_env();

    match args.command {
        SubCommand::Run(run_args) => {
            // Setup logging
            if run_args.log_to_stderr {
                simple_logger::init_with_level(Level::Info)?;
            } else {
                JournalLog::new()?.install()?;
            }

            let config = load_config(&args.config_file).await?;
            let cancel_token = CancellationToken::new();
            let cancel_token_clone = cancel_token.clone();
            
            // Spawn a task to handle Ctrl+C
            tokio::spawn(async move {
                if let Ok(()) = signal::ctrl_c().await {
                    log::info!("Terminate signal received. Initiating graceful shutdown...");
                    cancel_token_clone.cancel();
                }
            });
            
            // Retry loop with 60-second delay
            loop {
                // Check if cancellation was requested
                if cancel_token.is_cancelled() {
                    log::info!("Shutdown requested. Exiting...");
                    return Ok(());
                }

                match app::App::new(config.clone(), cancel_token.clone()).await {
                    Ok(mut app) => {
                        if let Err(error) = app.run().await {
                            log::error!("Fatal error: {error:#}");
                            log::error!("Restarting in 60 seconds...");
                            
                            // Wait for either 60 seconds or cancellation
                            tokio::select! {
                                _ = time::sleep(Duration::from_secs(60)) => {
                                    // Continue with restart
                                }
                                _ = cancel_token.cancelled() => {
                                    log::info!("Shutdown requested during restart delay. Exiting...");
                                    return Ok(());
                                }
                            }
                        } else {
                            break;
                        }
                    }
                    Err(error) => {
                        log::error!("Failed to initialize application: {error:#}");
                        log::error!("Restarting in 60 seconds...");
                        
                        // Wait for either 60 seconds or cancellation
                        tokio::select! {
                            _ = time::sleep(Duration::from_secs(60)) => {
                                // Continue with restart
                            }
                            _ = cancel_token.cancelled() => {
                                log::info!("Shutdown requested during restart delay. Exiting...");
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
        SubCommand::SetPassword(_) => {
            let config = load_config(&args.config_file).await?;
            crate::password::set_password(config).await?;
        }
    }

    Ok(())
}
