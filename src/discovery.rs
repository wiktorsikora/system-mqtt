use serde::Serialize;

#[derive(Serialize)]
pub struct SingleComponentDiscoveryPayload {
    pub unique_id: String,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_class: Option<String>,
    pub state_class: Option<String>,
    pub state_topic: String,
    pub value_template: String,
    pub unit_of_measurement: Option<String>,
    pub icon: Option<String>,
    pub device: Device,
}

#[derive(Serialize)]
pub struct Device {
    pub identifiers: Vec<String>,
    pub name: String,
}
