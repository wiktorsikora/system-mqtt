use anyhow::Context;
use lm_sensors::feature::Kind;
use lm_sensors::{LMSensors, Value};
use crate::home_assistant::HomeAssistant;

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

    pub async fn publish_values(&mut self, home_assistant: &HomeAssistant) -> anyhow::Result<()> {
        for chip in self.sensors.chip_iter(None) {
            for feature in chip.feature_iter() {
                let Some(feature_kind) = feature.kind() else {
                    log::warn!("Failed to get feature from chip: {:?}", chip.name());
                    continue;
                };

                let sensor_name = format!("{}_{}", chip.name()?, feature.label().unwrap_or("unknown".to_string()));
                let sensor_name = sensor_name.replace(" ", "-");

                for sub_feature in feature.sub_feature_iter() {
                    let val = sub_feature.value();

                    match feature_kind {
                        Kind::Voltage => {
                            if let Ok(Value::VoltageInput(voltage)) = val {
                                home_assistant.publish(&sensor_name, format!("{:.2}", voltage)).await;
                            }
                        }
                        Kind::Temperature => {
                            if let Ok(Value::TemperatureInput(temp)) = val {
                                home_assistant.publish(&sensor_name, format!("{:.2}", temp)).await;
                            }
                        }
                        Kind::Fan => {
                            if let Ok(Value::FanInput(fan)) = val {
                                home_assistant.publish(&sensor_name, format!("{:.2}", fan)).await;
                            }
                        }
                        Kind::Power => {
                            if let Ok(Value::PowerInput(power)) = val {
                                home_assistant.publish(&sensor_name, format!("{:.2}", power)).await;
                            }
                        }
                        Kind::Energy => {
                            if let Ok(Value::EnergyInput(energy)) = val {
                                home_assistant.publish(&sensor_name, format!("{:.2}", energy)).await;
                            }
                        }
                        Kind::Current => {
                            if let Ok(Value::CurrentInput(current)) = val {
                                home_assistant.publish(&sensor_name, format!("{:.2}", current)).await;
                            }
                        }
                        Kind::Humidity => {
                            if let Ok(Value::HumidityInput(humidity)) = val {
                                home_assistant.publish(&sensor_name, format!("{:.2}", humidity)).await;
                            }
                        }
                        _ => {
                            log::warn!("Unknown feature kind: {:?}", feature_kind);
                        }
                    }

                }
                //
                // if feature.kind() == Some(lm_sensors::feature::Kind::Temperature) {
                //     let sensor_name = format!("{}_{}", chip.name()?, feature.label().unwrap_or("unknown".to_string()));
                //     let sensor_name = sensor_name.replace(" ", "-");
                //
                //     for sub_feature in feature.sub_feature_iter() {
                //         if let Ok(Value::TemperatureInput(temp)) = sub_feature.value() {
                //             home_assistant.publish(&sensor_name, format!("{:.2}", temp)).await;
                //         }
                //     }
                // }
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

                let sensor_id = format!(
                    "{}_{}",
                    chip.name()?,
                    feature.label().unwrap_or("unknown".to_string())
                );
                // replace all spaces with dashes
                let sensor_id = sensor_id.replace(" ", "-");

                match feature_kind {
                    Kind::Voltage => {
                        home_assistant
                            .register_entity(
                                "sensor",
                                Some("voltage"),
                                Some("measurement"),
                                &sensor_id,
                                Some("V"),
                                Some("mdi:flash"),
                            )
                            .await
                            .context("Failed to register voltage sensor topic.")?;
                    }
                    Kind::Fan => {
                        home_assistant
                            .register_entity(
                                "sensor",
                                Some("fan"),
                                Some("measurement"),
                                &sensor_id,
                                Some("RPM"),
                                Some("mdi:fan"),
                            )
                            .await
                            .context("Failed to register fan sensor topic.")?;
                    }
                    Kind::Temperature => {
                        home_assistant
                            .register_entity(
                                "sensor",
                                Some("temperature"),
                                Some("measurement"),
                                &sensor_id,
                                Some("°C"),
                                Some("mdi:thermometer"),
                            )
                            .await
                            .context("Failed to register temperature sensor topic.")?;
                    }
                    Kind::Power => {
                        home_assistant
                            .register_entity(
                                "sensor",
                                Some("power"),
                                Some("measurement"),
                                &sensor_id,
                                Some("W"),
                                Some("mdi:flash"),
                            )
                            .await
                            .context("Failed to register power sensor topic.")?;
                    }
                    Kind::Energy => {
                        home_assistant
                            .register_entity(
                                "sensor",
                                Some("energy"),
                                Some("measurement"),
                                &sensor_id,
                                Some("kWh"),
                                Some("mdi:flash"),
                            )
                            .await
                            .context("Failed to register energy sensor topic.")?;
                    }
                    Kind::Current => {
                        home_assistant
                            .register_entity(
                                "sensor",
                                Some("current"),
                                Some("measurement"),
                                &sensor_id,
                                Some("A"),
                                Some("mdi:flash"),
                            )
                            .await
                            .context("Failed to register current sensor topic.")?;
                    }
                    Kind::Humidity => {
                        home_assistant
                            .register_entity(
                                "sensor",
                                Some("humidity"),
                                Some("measurement"),
                                &sensor_id,
                                Some("%"),
                                Some("mdi:water-percent"),
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