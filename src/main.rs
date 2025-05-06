mod cli;
mod config;
mod discovery;
mod home_assistant;
mod lm_sensors_impl;
mod mqtt;
mod password;
mod system_sensors;

use crate::cli::{Arguments, SubCommand};
use crate::config::{load_config, Config};
use crate::home_assistant::HomeAssistant;
use crate::lm_sensors_impl::SensorsImpl;
use crate::mqtt::{mqtt_loop, setup_mqtt_client};
use crate::password::set_password as password_set_password;
use crate::system_sensors::{collect_system_stats, register_system_sensors};
use anyhow::{Context, Result};
use rumqttc::ConnectionError;
use std::{collections::HashMap, path::PathBuf, time::Duration};
use sysinfo::System;
use systemd_journal_logger::JournalLog;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio::{signal, time};

#[tokio::main]
async fn main() {
    let arguments: Arguments = argh::from_env();

    match load_config(&arguments.config_file).await {
        Ok(config) => match arguments.command {
            SubCommand::Run(arguments) => {
                if arguments.log_to_stderr {
                    simple_logger::SimpleLogger::new()
                        .env()
                        .init()
                        .expect("Failed to setup log.");
                } else {
                    JournalLog::new().unwrap().install().unwrap();
                }

                while let Err(error) = application_trampoline(&config).await {
                    log::error!("Fatal error: {error:#}");
                    log::error!("Restarting in 60 seconds...");
                    time::sleep(Duration::from_secs(60)).await;
                }
            }
            SubCommand::SetPassword(_arguments) => {
                if let Err(error) = password_set_password(config).await {
                    eprintln!("Fatal error: {}", error);
                }
            }
        },
        Err(error) => {
            eprintln!("Failed to load config file: {}", error);
        }
    }
}

async fn application_trampoline(config: &Config) -> Result<()> {
    let mut system = System::new_all();

    let hostname = System::host_name().context("Could not get system hostname.")?;

    let device_id = config.unique_id.clone().unwrap_or_else(|| hostname);

    // Setup MQTT client using the mqtt module
    let (client, eventloop) = setup_mqtt_client(config, &device_id).await?;
    let manager = battery::Manager::new().context("Failed to initialize battery monitoring.")?;

    let mut home_assistant = HomeAssistant::new(device_id, client)?;

    // Register system sensors
    register_system_sensors(&mut home_assistant, config).await?;

    let mut sensors = SensorsImpl::new()?;
    sensors.register_sensors(&mut home_assistant).await?;

    home_assistant.set_available(true).await?;

    let mqtt_task = mqtt_loop(eventloop).await;

    let result = availability_trampoline(
        mqtt_task,
        &home_assistant,
        &mut system,
        config,
        manager,
        sensors,
    )
    .await;

    if let Err(error) = home_assistant.set_available(false).await {
        log::error!("Error while disconnecting from home assistant: {:#}", error);
    }

    result?;

    home_assistant.disconnect().await?;

    Ok(())
}

async fn availability_trampoline(
    mqtt_task: JoinHandle<std::result::Result<(), ConnectionError>>,
    home_assistant: &HomeAssistant,
    system: &mut System,
    config: &Config,
    manager: battery::Manager,
    mut sensors: SensorsImpl,
) -> Result<()> {
    let drive_list: HashMap<PathBuf, String> = config
        .drives
        .iter()
        .map(|drive_config| (drive_config.path.clone(), drive_config.name.clone()))
        .collect();

    system.refresh_all();

    // Run the main event loop
    run_event_loop(
        mqtt_task,
        home_assistant,
        system,
        config,
        &manager,
        &mut sensors,
        &drive_list,
    )
    .await
}

/// Run the main event loop for the application
async fn run_event_loop(
    mqtt_task: JoinHandle<std::result::Result<(), ConnectionError>>,
    home_assistant: &HomeAssistant,
    system: &mut System,
    config: &Config,
    manager: &battery::Manager,
    sensors: &mut SensorsImpl,
    drive_list: &HashMap<PathBuf, String>,
) -> Result<()> {
    let mut discovery_interval = tokio::time::interval_at(
        Instant::now(),
        config
            .discovery_interval
            .unwrap_or(Duration::from_secs(60 * 60)),
    );
    let mut interval = tokio::time::interval_at(Instant::now(), config.update_interval);

    // create fused mqtt_task
    use futures_util::future::FutureExt;
    let mut mqtt_fut = mqtt_task.fuse();

    loop {
        tokio::select! {
            result = &mut mqtt_fut => {
                match result {
                    Ok(Ok(_)) => {
                        log::info!("MQTT task completed successfully, exiting.");
                        break;
                    }
                    Ok(Err(e)) => {
                        log::error!("MQTT task failed: {:#}", e);
                        return Err(e).context("MQTT task failed.");
                    }
                    Err(e) => {
                        log::error!("MQTT task failed: {:#}", e);
                        return Err(e).context("MQTT task failed.");
                    }
                }
            }
            _ = discovery_interval.tick() => {
                home_assistant.publish_discovery().await?
            }
            _ = interval.tick() => {
                // Collect system statistics using the stats module
                let stats = collect_system_stats(system, drive_list, manager, sensors).await?;

                // Serialize stats to JSON and publish.
                let json_message = serde_json::to_string(&stats).context("Failed to serialize stats to JSON.")?;
                home_assistant.publish("state", json_message).await;
            }
            _ = signal::ctrl_c() => {
                log::info!("Terminate signal has been received.");
                break;
            }
        }
    }

    Ok(())
}
