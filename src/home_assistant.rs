use mqtt_async_client::client::{Client as MqttClient, Publish};

use std::collections::HashSet;
use anyhow::Context;
use crate::discovery::{Device, DiscoveryPayload};

pub struct HomeAssistant {
    client: MqttClient,
    hostname: String,
    registered_topics: HashSet<String>,
}

impl HomeAssistant {
    pub fn new(hostname: String, client: MqttClient) -> anyhow::Result<Self> {
        let home_assistant = Self {
            client,
            hostname,
            registered_topics: HashSet::new(),
        };

        Ok(home_assistant)
    }
    pub async fn set_available(&self, available: bool) -> anyhow::Result<()> {
        self.client
            .publish(
                Publish::new(
                    format!("system-mqtt/{}/availability", self.hostname),
                    if available { "online" } else { "offline" }.into(),
                )
                    .set_retain(true),
            )
            .await
            .context("Failed to publish availability topic.")
    }

    pub async fn register_entity(
        &mut self,
        topic_class: &str,
        device_class: Option<&str>,
        state_class: Option<&str>,
        entity_id: &str,
        unit_of_measurement: Option<&str>,
        icon: Option<&str>,
    ) -> anyhow::Result<()> {
        log::info!("Registering entity `{}`.", entity_id);

        let message = serde_json::ser::to_string(&DiscoveryPayload {
            unique_id: format!("{}-{}", self.hostname, entity_id),
            device: Device {
                identifiers: vec![self.hostname.clone()],
                name: self.hostname.clone(),
            },
            name: format!("{}-{}", self.hostname, entity_id),
            device_class: device_class.map(str::to_string),
            state_class: state_class.map(str::to_string),
            state_topic: format!("system-mqtt/{}/{}", self.hostname, entity_id),
            unit_of_measurement: unit_of_measurement.map(str::to_string),
            icon: icon.map(str::to_string),
        })
            .context("Failed to serialize topic information.")?;
        let mut publish = Publish::new(
            format!(
                "homeassistant/{}/system-mqtt-{}/{}/config",
                topic_class, self.hostname, entity_id
            ),
            message.into(),
        );
        publish.set_retain(false);
        self.client
            .publish(&publish)
            .await
            .context("Failed to publish topic to MQTT server.")?;

        self.registered_topics.insert(entity_id.to_string());

        Ok(())
    }

    pub async fn publish(&self, topic_name: &str, value: String) {
        log::debug!("PUBLISH `{}` TO `{}`", value, topic_name);

        if self.registered_topics.contains(topic_name) {
            let mut publish = Publish::new(
                format!("system-mqtt/{}/{}", self.hostname, topic_name),
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
