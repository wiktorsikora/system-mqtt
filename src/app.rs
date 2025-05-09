use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio::time;
use sysinfo::System;
use battery::Manager;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::home_assistant::HomeAssistant;
use crate::lm_sensors_impl::SensorsImpl;
use crate::nvidia_gpu::NvidiaGpuSensors;
use crate::system_sensors::{collect_system_stats, register_system_sensors};

/// Main application structure that manages the System MQTT daemon.
/// 
/// This struct coordinates all the components of the system monitoring daemon,
/// including system statistics collection, MQTT communication, and sensor management.
pub struct App {
    config: Config,
    system: System,
    home_assistant: HomeAssistant,
    sensors: SensorsImpl,
    gpu_sensors: NvidiaGpuSensors,
    battery_manager: Manager,
    drive_list: HashMap<PathBuf, String>,
    mqtt_task: JoinHandle<std::result::Result<(), rumqttc::ConnectionError>>,
    cancel_token: CancellationToken,
}

impl App {
    /// Create a new instance of the System MQTT daemon.
    /// 
    /// This initializes all components including:
    /// - System monitoring
    /// - MQTT client
    /// - Home Assistant integration
    /// - Hardware sensors
    /// - Battery monitoring
    /// 
    /// # Arguments
    /// 
    /// * `config` - The configuration for the daemon
    /// * `cancel_token` - Token used for graceful shutdown
    /// 
    /// # Returns
    /// 
    /// A new App instance ready to run, or an error if initialization fails.
    pub async fn new(config: Config, cancel_token: CancellationToken) -> Result<Self> {
        let mut system = System::new_all();
        let hostname = System::host_name().context("Could not get system hostname.")?;
        let device_id = config.unique_id.clone().unwrap_or_else(|| hostname);

        // Setup MQTT client
        let (client, eventloop) = crate::mqtt::setup_mqtt_client(&config, &device_id).await?;
        let manager = battery::Manager::new().context("Failed to initialize battery monitoring.")?;

        let mut home_assistant = HomeAssistant::new(device_id, client)?;

        // Register system sensors
        register_system_sensors(&mut home_assistant, &config).await?;

        let mut sensors = SensorsImpl::new()?;
        sensors.register_sensors(&mut home_assistant).await?;

        let mut gpu_sensors = NvidiaGpuSensors::new();
        gpu_sensors.init().await?;
        gpu_sensors.register_sensors(&mut home_assistant).await?;

        home_assistant.set_available(true).await?;

        let mqtt_task = crate::mqtt::mqtt_loop(eventloop).await;

        let drive_list: HashMap<PathBuf, String> = config
            .drives
            .iter()
            .map(|drive_config| (drive_config.path.clone(), drive_config.name.clone()))
            .collect();

        system.refresh_all();

        Ok(Self {
            config,
            system,
            home_assistant,
            sensors,
            gpu_sensors,
            battery_manager: manager,
            drive_list,
            mqtt_task,
            cancel_token,
        })
    }

    /// Run the main daemon loop.
    /// 
    /// This method runs the main loop that:
    /// - Collects system statistics at configured intervals
    /// - Publishes updates to MQTT
    /// - Sends Home Assistant discovery messages
    /// - Handles graceful shutdown
    /// 
    /// The loop continues until either:
    /// - The MQTT connection fails
    /// - A shutdown signal is received
    /// - An unrecoverable error occurs
    /// 
    /// # Returns
    /// 
    /// Returns Ok(()) if the daemon shuts down gracefully, or an error if something goes wrong.
    pub async fn run(&mut self) -> Result<()> {
        let mut discovery_interval = time::interval_at(
            Instant::now(),
            self.config
                .discovery_interval
                .unwrap_or(Duration::from_secs(60 * 60)),
        );
        let mut update_interval = time::interval_at(Instant::now(), self.config.update_interval);

        loop {
            tokio::select! {
                result = &mut self.mqtt_task => {
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
                    self.home_assistant.publish_discovery().await?
                }
                _ = update_interval.tick() => {
                    let stats = collect_system_stats(
                        &mut self.system,
                        &self.drive_list,
                        &self.battery_manager,
                        &mut self.sensors,
                        &self.gpu_sensors,
                    ).await?;

                    let json_message = serde_json::to_string(&stats)
                        .context("Failed to serialize stats to JSON.")?;
                    self.home_assistant.publish("state", json_message).await;
                }
                _ = self.cancel_token.cancelled() => {
                    log::info!("Shutdown signal received, exiting...");
                    break;
                }
            }
        }

        self.cleanup().await?;
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        if let Err(error) = self.home_assistant.set_available(false).await {
            log::error!("Error while disconnecting from home assistant: {:#}", error);
        }
        self.home_assistant.disconnect().await?;
        Ok(())
    }
} 