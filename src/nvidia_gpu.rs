use std::collections::HashMap;
use serde::Serialize;
use std::process::Stdio;
use tokio::{io::{AsyncBufReadExt, BufReader}, process::Command};
use anyhow::{Result};
use crate::home_assistant::{EntityRegistrationBuilder, HomeAssistant};
use crate::utils::sanitize_sensor_name;

#[derive(Debug, Serialize)]
pub struct GpuInfo {
    pub index: u32,
    pub name: String,
    pub temperature: u32,
    pub utilization: u32,
    pub memory_total: u32,
    pub memory_used: u32,
    pub memory_free: u32,
    pub power_draw: f64,
}


pub struct NvidiaGpuSensors {
    nvidia_smi_available: bool,
}

impl NvidiaGpuSensors {
    pub fn new() -> Self {
        Self {
            nvidia_smi_available: false,
        }
    }

    pub async fn init(&mut self) -> Result<()> {
        let gpu_info = get_nvidia_gpu_info().await;
        match gpu_info {
            Ok(gpu_info) => {
                self.nvidia_smi_available = true;
                log::debug!("NVIDIA GPU info: {:?}", gpu_info);
            }
            Err(err) => {
                log::debug!("Failed to get NVIDIA GPU info, nvidia sensors disabled: {err:#}");
            }
        }
        Ok(())
    }

    pub async fn collect_values(&self, stats: &mut HashMap<String, serde_json::Value>) -> Result<()> {
        if !self.nvidia_smi_available {
            return Ok(());
        }
        let gpu_info = get_nvidia_gpu_info().await?;
        for gpu in gpu_info {
            let gpu_name = sanitize_sensor_name(gpu.name);
            stats.insert(format!("gpu_{}_temperature", gpu_name), serde_json::Value::from(gpu.temperature));
            stats.insert(format!("gpu_{}_utilization", gpu_name), serde_json::Value::from(gpu.utilization));
            stats.insert(format!("gpu_{}_memory_used", gpu_name), serde_json::Value::from(gpu.memory_used));
            stats.insert(format!("gpu_{}_power_draw", gpu_name), serde_json::Value::from(gpu.power_draw));
        }
        Ok(())
    }

    pub async fn register_sensors(&self, home_assistant: &mut HomeAssistant) -> Result<()> {
        if !self.nvidia_smi_available {
            return Ok(());
        }
        let gpu_info = get_nvidia_gpu_info().await?;
        for gpu in gpu_info {
            let gpu_name = sanitize_sensor_name(gpu.name);
            home_assistant
                .register_entity_with_builder(
                    EntityRegistrationBuilder::new("sensor", &format!("gpu_{}_temperature", gpu_name))
                        .unit_of_measurement("°C")
                        .icon("mdi:thermometer")
                )
                .await?;
            home_assistant
                .register_entity_with_builder(
                    EntityRegistrationBuilder::new("sensor", &format!("gpu_{}_utilization", gpu_name))
                        .unit_of_measurement("%")
                        .icon("mdi:percent")
                )
                .await?;
            home_assistant
                .register_entity_with_builder(
                    EntityRegistrationBuilder::new("sensor", &format!("gpu_{}_memory_used", gpu_name))
                        .unit_of_measurement("MB")
                        .icon("mdi:memory")
                )
                .await?;

            home_assistant
                .register_entity_with_builder(
                    EntityRegistrationBuilder::new("sensor", &format!("gpu_{}_power", gpu_name))
                        .unit_of_measurement("W")
                        .icon("mdi:flash")
                )
                .await?;
        }
        Ok(())
    }
}

pub async fn get_nvidia_gpu_info() -> Result<Vec<GpuInfo>> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,temperature.gpu,utilization.gpu,memory.total,memory.used,memory.free,power.draw",
            "--format=csv,noheader,nounits",
        ])
        .stdout(Stdio::piped())
        .spawn()?;

    let stdout = output.stdout.ok_or_else(|| anyhow::anyhow!("failed to capture stdout"))?;
    let reader = BufReader::new(stdout).lines();

    let mut gpu_info_list = Vec::new();

    tokio::pin!(reader);

    while let Some(line) = reader.next_line().await? {
        let fields: Vec<&str> = line.trim().split(',').map(str::trim).collect();
        if fields.len() != 8 {
            continue; // skip malformed lines
        }

        let info = GpuInfo {
            index: fields[0].parse()?,
            name: fields[1].to_string(),
            temperature: fields[2].parse()?,
            utilization: fields[3].parse()?,
            memory_total: fields[4].parse()?,
            memory_used: fields[5].parse()?,
            memory_free: fields[6].parse()?,
            power_draw: fields[7].parse()?,
        };

        gpu_info_list.push(info);
    }

    Ok(gpu_info_list)
}
