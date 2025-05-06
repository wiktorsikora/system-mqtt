use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use sysinfo::{CpuExt, DiskExt, System, SystemExt};
use crate::config::Config;
use crate::home_assistant::{EntityRegistrationBuilder, HomeAssistant};
use crate::lm_sensors_impl::SensorsImpl;

/// Register all system sensors with Home Assistant
pub async fn register_system_sensors(home_assistant: &mut HomeAssistant, config: &Config) -> Result<()> {
    // Register the various sensor topics and include the details about that sensor
    home_assistant
        .register_entity_with_builder(
            EntityRegistrationBuilder::new("sensor", "available")
                .icon("mdi:check-network-outline")
        )
        .await
        .context("Failed to register availability topic.")?;
    home_assistant
        .register_entity_with_builder(
            EntityRegistrationBuilder::new("sensor", "uptime")
                .unit_of_measurement("days")
                .icon("mdi:timer-sand")
        )
        .await
        .context("Failed to register uptime topic.")?;
    home_assistant
        .register_entity_with_builder(
            EntityRegistrationBuilder::new("sensor", "cpu")
                .state_class("measurement")
                .unit_of_measurement("%")
                .icon("mdi:gauge")
        )
        .await
        .context("Failed to register CPU usage topic.")?;
    home_assistant
        .register_entity_with_builder(
            EntityRegistrationBuilder::new("sensor", "memory")
                .state_class("measurement")
                .unit_of_measurement("%")
                .icon("mdi:gauge")
        )
        .await
        .context("Failed to register memory usage topic.")?;
    home_assistant
        .register_entity_with_builder(
            EntityRegistrationBuilder::new("sensor", "swap")
                .state_class("measurement")
                .unit_of_measurement("%")
                .icon("mdi:gauge")
        )
        .await
        .context("Failed to register swap usage topic.")?;
    home_assistant
        .register_entity_with_builder(
            EntityRegistrationBuilder::new("sensor", "battery_level")
                .device_class("battery")
                .state_class("measurement")
                .unit_of_measurement("%")
                .icon("mdi:battery")
        )
        .await
        .context("Failed to register battery level topic.")?;
    home_assistant
        .register_entity_with_builder(
            EntityRegistrationBuilder::new("sensor", "battery_state")
                .icon("mdi:battery")
        )
        .await
        .context("Failed to register battery state topic.")?;

    // Register the sensors for filesystems
    for drive in &config.drives {
        home_assistant
            .register_entity_with_builder(
                EntityRegistrationBuilder::new("sensor", &drive.name)
                    .state_class("total")
                    .unit_of_measurement("%")
                    .icon("mdi:folder")
            )
            .await
            .context("Failed to register a filesystem topic.")?;
    }

    Ok(())
}

/// Collect system statistics and store them in the provided HashMap
pub async fn collect_system_stats(
    system: &mut System,
    drive_list: &HashMap<PathBuf, String>,
    manager: &battery::Manager,
    sensors: &mut SensorsImpl,
) -> Result<HashMap<String, Value>> {
    // Refresh system information
    system.refresh_disks();
    system.refresh_memory();
    system.refresh_cpu();

    let mut stats = HashMap::new();

    // Collect uptime.
    let uptime = system.uptime() as f32 / 60.0 / 60.0 / 24.0; // Convert from seconds to days.
    stats.insert("uptime".to_string(), Value::from(uptime));

    // Collect CPU usage.
    let cpu_usage = (system.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>()) / (system.cpus().len() as f32 * 100.0);
    stats.insert("cpu".to_string(), Value::from(cpu_usage * 100.0));

    // Collect memory usage.
    let memory_percentile = (system.total_memory() - system.available_memory()) as f64 / system.total_memory() as f64;
    stats.insert("memory".to_string(), Value::from(memory_percentile.clamp(0.0, 1.0) * 100.0));

    // Collect swap usage.
    let total_swap = system.used_swap() + system.free_swap();
    let swap_percentile = if total_swap > 0 {
        system.used_swap() as f64 / total_swap as f64
    } else {
        0.0
    };
    stats.insert("swap".to_string(), Value::from(swap_percentile.clamp(0.0, 1.0) * 100.0));

    // Collect filesystem usage.
    for drive in system.disks() {
        if let Some(drive_name) = drive_list.get(drive.mount_point()) {
            let drive_percentile = (drive.total_space() - drive.available_space()) as f64 / drive.total_space() as f64;
            stats.insert(drive_name.clone(), Value::from(drive_percentile.clamp(0.0, 1.0) * 100.0));
        }
    }

    // Collect battery information.
    if let Some(battery) = manager.batteries().context("Failed to read battery info.")?.flatten().next() {
        use battery::State;

        let battery_state = match battery.state() {
            State::Charging => "charging",
            State::Discharging => "discharging",
            State::Empty => "empty",
            State::Full => "full",
            _ => "unknown",
        };
        stats.insert("battery_state".to_string(), Value::from(battery_state));

        let battery_full = battery.energy_full();
        let battery_power = battery.energy();
        let battery_level = battery_power / battery_full;

        stats.insert("battery_level".to_string(), Value::from(battery_level.value));
    }

    // Collect lm_sensors data.
    sensors.collect_values(&mut stats).await?;

    Ok(stats)
}