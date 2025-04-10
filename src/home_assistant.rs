use mqtt_async_client::client::{Client as MqttClient, Publish};

use std::collections::HashSet;
use anyhow::Context;
use crate::discovery::{Device, SingleComponentDiscoveryPayload};


pub struct HomeAssistant {
    client: MqttClient,
    device_id: String,
    registered_topics: HashSet<String>,
    discovery_info: Vec<(String, SingleComponentDiscoveryPayload)>
}

impl HomeAssistant {
    pub fn new(device_id: String, client: MqttClient) -> anyhow::Result<Self> {
        let home_assistant = Self {
            client,
            device_id,
            registered_topics: HashSet::new(),
            discovery_info: vec![],
        };

        Ok(home_assistant)
    }
    pub async fn set_available(&self, available: bool) -> anyhow::Result<()> {
        self.client
            .publish(
                Publish::new(
                    format!("system-mqtt/{}/availability", self.device_id),
                    if available { "online" } else { "offline" }.into(),
                )
                    .set_retain(true),
            )
            .await
            .context("Failed to publish availability topic.")
    }

    pub async fn register_entity(
        &mut self,
        platform: &str,
        device_class: Option<&str>,
        state_class: Option<&str>,
        entity_id: &str,
        unit_of_measurement: Option<&str>,
        icon: Option<&str>,
    ) -> anyhow::Result<()> {
        log::info!("Registering entity `{}`.", entity_id);

        let topic = format!("system-mqtt/{}/state", self.device_id);
        let payload = SingleComponentDiscoveryPayload {
            unique_id: format!("{}-{}", self.device_id, entity_id),
            device: Device {
                identifiers: vec![self.device_id.clone()],
                name: self.device_id.clone(),
            },
            name: format!("{}-{}", self.device_id, entity_id),
            device_class: device_class.map(str::to_string),
            state_class: state_class.map(str::to_string),
            state_topic: topic.clone(),
            value_template: format!(r"{{{{ value_json['{entity_id}'] }}}}"),
            unit_of_measurement: unit_of_measurement.map(str::to_string),
            icon: icon.map(str::to_string),
        };

        let discovery_topic = format!(
            "homeassistant/{}/system-mqtt-{}/{}/config",
            platform, self.device_id, entity_id
        );
        self.discovery_info.push((discovery_topic.clone(), payload));
        self.registered_topics.insert(topic);
        Ok(())
    }

    pub async fn publish_discovery(&self) -> anyhow::Result<()> {
        for (topic, payload) in &self.discovery_info {
            let message = serde_json::ser::to_string(payload)
                .context("Failed to serialize topic information.")?;
            let publish = Publish::new(
                topic.clone(),
                message.into(),
            );
            // publish.set_retain(true);
            self.client
                .publish(&publish)
                .await
                .context("Failed to publish topic to MQTT server.")?;
        }

        Ok(())
    }

    pub async fn publish(&self, topic_name: &str, value: String) {
        log::debug!("PUBLISH `{}` TO `{}`", value, topic_name);

        let topic = format!("system-mqtt/{}/{}", self.device_id, topic_name);
        if self.registered_topics.contains(&topic) {
            let mut publish = Publish::new(
                topic,
                value.into(),
            );
            publish.set_retain(false);

            if let Err(error) = self.client.publish(&publish).await {
                log::error!("Failed to publish topic `{}`: {:?}", topic_name, error);
            }
        } else {
            log::error!(
                "Attempt to publish topic `{}`, which was never registered with Home Assistant.",
                topic_name
            );
        }
    }

    pub async fn disconnect(mut self) -> anyhow::Result<()> {
        self.set_available(false).await?;
        self.client.disconnect().await?;

        Ok(())
    }
}
