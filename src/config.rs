use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use url::Url;

pub async fn load_config(path: &Path) -> anyhow::Result<Config> {
    if path.is_file() {
        // It's a readable file we can load.

        let config: Config = serde_yaml::from_str(&fs::read_to_string(path).await?)
            .context("Failed to deserialize config file.")?;

        Ok(config)
    } else {
        log::info!("No config file present. A default one will be written.");
        // Doesn't exist yet. We'll create it.
        let config = Config::default();

        // Write it to a file for next time we load.
        fs::write(path, serde_yaml::to_string(&config)?).await?;

        Ok(config)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {

    /// The unique ID of the device.
    /// If not specified, the hostname will be used.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,

    /// The URL of the mqtt server.
    pub mqtt_server: Url,

    /// Set the username to connect to the mqtt server, if required.
    /// The password will be fetched from the OS keyring.
    pub username: Option<String>,

    /// Where the password for the MQTT server can be found.
    /// If a username is not specified, this field is ignored.
    /// If not specified, this field defaults to the keyring.
    #[serde(default)]
    pub password_source: PasswordSource,

    /// The interval to update at.
    pub update_interval: Duration,

    /// The interval to send discovery messages at.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_interval: Option<Duration>,

    /// The names of drives, or the paths to where they are mounted.
    pub drives: Vec<DriveConfig>,

    /// The path to the CA certificate for the MQTT server.
    /// This is only required if the server uses a self-signed certificate.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_cert: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        // This URL parsing should never fail as it's a hardcoded, valid URL
        let mqtt_server = Url::parse("mqtt://localhost")
            .unwrap_or_else(|_| panic!("Failed to parse default URL, this is a bug"));

        Self {
            unique_id: None,
            mqtt_server,
            username: None,
            password_source: PasswordSource::Keyring,
            update_interval: Duration::from_secs(30),
            discovery_interval: Some(Duration::from_secs(60 * 60)),
            drives: vec![DriveConfig {
                path: PathBuf::from("/"),
                name: String::from("root"),
            }],
            ca_cert: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DriveConfig {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum PasswordSource {
    #[serde(rename = "keyring")]
    Keyring,

    #[serde(rename = "secret_file")]
    SecretFile(PathBuf),

    #[serde(rename = "plaintext")]
    Plaintext(String),
}

impl Default for PasswordSource {
    fn default() -> Self {
        Self::Keyring
    }
}
