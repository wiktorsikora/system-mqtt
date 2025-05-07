use rumqttc::{AsyncClient, QoS};
use std::collections::HashSet;
use anyhow::{Context, Result, bail};
use crate::discovery::{Device, SingleComponentDiscoveryPayload};

/// Builder for entity registration parameters
pub struct EntityRegistrationBuilder<'a> {
    platform: &'a str,
    device_class: Option<&'a str>,
    state_class: Option<&'a str>,
    entity_id: &'a str,
    unit_of_measurement: Option<&'a str>,
    icon: Option<&'a str>,
}

impl<'a> EntityRegistrationBuilder<'a> {
    pub fn new(platform: &'a str, entity_id: &'a str) -> Self {
        Self {
            platform,
            device_class: None,
            state_class: None,
            entity_id,
            unit_of_measurement: None,
            icon: None,
        }
    }

    pub fn device_class(mut self, device_class: &'a str) -> Self {
        self.device_class = Some(device_class);
        self
    }

    pub fn state_class(mut self, state_class: &'a str) -> Self {
        self.state_class = Some(state_class);
        self
    }

    pub fn unit_of_measurement(mut self, unit: &'a str) -> Self {
        self.unit_of_measurement = Some(unit);
        self
    }

    pub fn icon(mut self, icon: &'a str) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// Validates that an entity ID contains only valid characters
/// Entity IDs should only contain lowercase alphanumeric characters and underscores
fn validate_entity_id(entity_id: &str) -> Result<()> {
    if entity_id.is_empty() {
        bail!("Entity ID cannot be empty");
    }

    // Check if entity_id contains only lowercase alphanumeric characters and underscores
    if !entity_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        bail!("Entity ID '{}' contains invalid characters", entity_id);
    }

    Ok(())
}

pub struct HomeAssistant {
    client: AsyncClient,
    device_id: String,
    registered_topics: HashSet<String>,
    discovery_info: Vec<(String, SingleComponentDiscoveryPayload)>
}

impl HomeAssistant {
    pub fn new(device_id: String, client: AsyncClient) -> Result<Self> {
        let home_assistant = Self {
            client,
            device_id,
            registered_topics: HashSet::new(),
            discovery_info: vec![],
        };

        Ok(home_assistant)
    }

    pub async fn set_available(&self, available: bool) -> Result<()> {
        let payload = if available { "online" } else { "offline" };
        self.client
            .publish(
                format!("system-mqtt/{}/availability", self.device_id),
                QoS::AtLeastOnce,
                true,
                payload,
            )
            .await
            .context("Failed to publish availability topic.")
    }

    /// Register an entity using the builder pattern
    pub async fn register_entity_with_builder(
        &mut self,
        builder: EntityRegistrationBuilder<'_>,
    ) -> Result<()> {
        // Validate the entity ID before proceeding
        validate_entity_id(builder.entity_id)?;

        log::info!("Registering entity `{}`.", builder.entity_id);

        let topic = format!("system-mqtt/{}/state", self.device_id);
        let payload = SingleComponentDiscoveryPayload {
            unique_id: format!("{}-{}", self.device_id, builder.entity_id),
            device: Device {
                identifiers: vec![self.device_id.clone()],
                name: self.device_id.clone(),
            },
            name: format!("{}-{}", self.device_id, builder.entity_id),
            device_class: builder.device_class.map(str::to_string),
            state_class: builder.state_class.map(str::to_string),
            state_topic: topic.clone(),
            value_template: format!(r"{{{{ value_json['{entity_id}'] }}}}", entity_id = builder.entity_id),
            unit_of_measurement: builder.unit_of_measurement.map(str::to_string),
            icon: builder.icon.map(str::to_string),
        };

        let discovery_topic = format!(
            "homeassistant/{}/system-mqtt-{}/{}/config",
            builder.platform, self.device_id, builder.entity_id
        );
        self.discovery_info.push((discovery_topic.clone(), payload));
        self.registered_topics.insert(topic);
        Ok(())
    }

    pub async fn publish_discovery(&self) -> Result<()> {
        for (topic, payload) in &self.discovery_info {
            let message = serde_json::ser::to_string(payload)
                .context("Failed to serialize topic information.")?;
            self.client
                .publish(topic.clone(), QoS::AtLeastOnce, true, message)
                .await
                .context("Failed to publish topic to MQTT server.")?;
        }

        Ok(())
    }

    pub async fn publish(&self, topic_name: &str, value: String) {
        log::debug!("PUBLISH `{}` TO `{}`", value, topic_name);

        let topic = format!("system-mqtt/{}/{}", self.device_id, topic_name);
        if self.registered_topics.contains(&topic) {
            if let Err(error) = self.client.publish(topic, QoS::AtLeastOnce, false, value).await {
                log::error!("Failed to publish topic `{}`: {:#}", topic_name, error);
            }
        } else {
            log::error!(
                "Attempt to publish topic `{}`, which was never registered with Home Assistant.",
                topic_name
            );
        }
    }

    pub async fn disconnect(self) -> Result<()> {
        self.set_available(false).await?;
        Ok(())
    }
}
