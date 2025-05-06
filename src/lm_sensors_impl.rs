use std::collections::HashMap;
use anyhow::Context;
use lm_sensors::feature::Kind;
use lm_sensors::{LMSensors, Value};
use crate::home_assistant::{HomeAssistant, EntityRegistrationBuilder};

/// Sanitize a sensor name by replacing spaces with dashes
fn sanitize_sensor_name(name: String) -> String {
    name.replace(" ", "-")
}

pub struct SensorsImpl {
    pub sensors: LMSensors,
}

impl SensorsImpl {
    pub fn new() -> anyhow::Result<Self> {
        let sensors = lm_sensors::Initializer::default().initialize()?;

        Ok(Self {
            sensors,
        })
    }

    pub async fn collect_values(&mut self, stats: &mut HashMap<String, serde_json::Value>) -> anyhow::Result<()> {
        for chip in self.sensors.chip_iter(None) {
            for feature in chip.feature_iter() {
                let Some(feature_kind) = feature.kind() else {
                    log::warn!("Failed to get feature from chip: {:?}", chip.name());
                    continue;
                };

                let sensor_name = sanitize_sensor_name(format!(
                    "{}_{}",
                    chip.name()?,
                    feature.label().unwrap_or("unknown".to_string())
                ));

                for sub_feature in feature.sub_feature_iter() {
                    let val = sub_feature.value();

                    match feature_kind {
                        Kind::Voltage => {
                            if let Ok(Value::VoltageInput(v)) = val {
                                stats.insert(sensor_name.clone(), serde_json::Value::from(v));
                            }
                        }
                        Kind::Temperature => {
                            if let Ok(Value::TemperatureInput(v)) = val {
                                stats.insert(sensor_name.clone(), serde_json::Value::from(v));
                            }
                        }
                        Kind::Fan => {
                            if let Ok(Value::FanInput(v)) = val {
                                stats.insert(sensor_name.clone(), serde_json::Value::from(v));
                            }
                        }
                        Kind::Power => {
                            if let Ok(Value::PowerInput(v)) = val {
                                stats.insert(sensor_name.clone(), serde_json::Value::from(v));
                            }
                        }
                        Kind::Energy => {
                            if let Ok(Value::EnergyInput(v)) = val {
                                stats.insert(sensor_name.clone(), serde_json::Value::from(v));
                            }
                        }
                        Kind::Current => {
                            if let Ok(Value::CurrentInput(v)) = val {
                                stats.insert(sensor_name.clone(), serde_json::Value::from(v));
                            }
                        }
                        Kind::Humidity => {
                            if let Ok(Value::HumidityInput(v)) = val {
                                stats.insert(sensor_name.clone(), serde_json::Value::from(v));
                            }
                        }
                        _ => {
                            log::warn!("Unknown feature kind: {:?}", feature_kind);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn print_sensors(&mut self) -> anyhow::Result<()> {
        for chip in self.sensors.chip_iter(None) {
            println!("Chip: {:?}", chip.name());
            for feature in chip.feature_iter() {
                println!("  Feature: {:?}", feature.label());
                for sub_feature in feature.sub_feature_iter() {
                    println!("    Subfeature: {:?}", sub_feature.name());
                    println!("      Kind: {:?}", sub_feature.kind());
                    println!("      Value: {:?}", sub_feature.value());
                }
            }
        }
        Ok(())
    }

    pub async fn register_sensors(&mut self, home_assistant: &mut HomeAssistant) -> anyhow::Result<()>{

        for chip in self.sensors.chip_iter(None) {
            for feature in chip.feature_iter() {
                let Some(feature_kind) = feature.kind() else {
                    log::warn!("Failed to get feature from chip: {:?}", chip.name());
                    continue;
                };

                let sensor_id = sanitize_sensor_name(format!(
                    "{}_{}",
                    chip.name()?,
                    feature.label().unwrap_or("unknown".to_string())
                ));

                match feature_kind {
                    Kind::Voltage => {
                        home_assistant
                            .register_entity_with_builder(
                                EntityRegistrationBuilder::new("sensor", &sensor_id)
                                    .device_class("voltage")
                                    .state_class("measurement")
                                    .unit_of_measurement("V")
                                    .icon("mdi:flash")
                            )
                            .await
                            .context("Failed to register voltage sensor topic.")?;
                    }
                    Kind::Fan => {
                        home_assistant
                            .register_entity_with_builder(
                                EntityRegistrationBuilder::new("sensor", &sensor_id)
                                    .device_class("fan")
                                    .state_class("measurement")
                                    .unit_of_measurement("RPM")
                                    .icon("mdi:fan")
                            )
                            .await
                            .context("Failed to register fan sensor topic.")?;
                    }
                    Kind::Temperature => {
                        home_assistant
                            .register_entity_with_builder(
                                EntityRegistrationBuilder::new("sensor", &sensor_id)
                                    .device_class("temperature")
                                    .state_class("measurement")
                                    .unit_of_measurement("°C")
                                    .icon("mdi:thermometer")
                            )
                            .await
                            .context("Failed to register temperature sensor topic.")?;
                    }
                    Kind::Power => {
                        home_assistant
                            .register_entity_with_builder(
                                EntityRegistrationBuilder::new("sensor", &sensor_id)
                                    .device_class("power")
                                    .state_class("measurement")
                                    .unit_of_measurement("W")
                                    .icon("mdi:flash")
                            )
                            .await
                            .context("Failed to register power sensor topic.")?;
                    }
                    Kind::Energy => {
                        home_assistant
                            .register_entity_with_builder(
                                EntityRegistrationBuilder::new("sensor", &sensor_id)
                                    .device_class("energy")
                                    .state_class("measurement")
                                    .unit_of_measurement("kWh")
                                    .icon("mdi:flash")
                            )
                            .await
                            .context("Failed to register energy sensor topic.")?;
                    }
                    Kind::Current => {
                        home_assistant
                            .register_entity_with_builder(
                                EntityRegistrationBuilder::new("sensor", &sensor_id)
                                    .device_class("current")
                                    .state_class("measurement")
                                    .unit_of_measurement("A")
                                    .icon("mdi:flash")
                            )
                            .await
                            .context("Failed to register current sensor topic.")?;
                    }
                    Kind::Humidity => {
                        home_assistant
                            .register_entity_with_builder(
                                EntityRegistrationBuilder::new("sensor", &sensor_id)
                                    .device_class("humidity")
                                    .state_class("measurement")
                                    .unit_of_measurement("%")
                                    .icon("mdi:water-percent")
                            )
                            .await
                            .context("Failed to register humidity sensor topic.")?;
                    }
                    Kind::VoltageID => {}
                    Kind::Intrusion => {}
                    Kind::BeepEnable => {}
                    Kind::Unknown => {}
                    _ => {
                        log::warn!("Unknown feature kind: {:?}", feature_kind);
                    }
                }
            }
        }
        Ok(())
    }

}