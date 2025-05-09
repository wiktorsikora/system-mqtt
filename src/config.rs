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

/// Configuration for the System MQTT daemon.
/// 
/// This struct contains all the settings needed to run the System MQTT daemon,
/// including MQTT server connection details, update intervals, and monitored drives.
#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    /// The unique ID of the device.
    /// If not specified, the hostname will be used.
    /// This ID is used to identify the device in Home Assistant and MQTT topics.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_id: Option<String>,

    /// The URL of the MQTT server to connect to.
    /// Format: `mqtt://hostname:port` or `mqtts://hostname:port` for secure connections.
    pub mqtt_server: Url,

    /// Set the username to connect to the MQTT server, if required.
    /// The password will be fetched from the OS keyring or other configured source.
    pub username: Option<String>,

    /// Where the password for the MQTT server can be found.
    /// If a username is not specified, this field is ignored.
    /// If not specified, this field defaults to the keyring.
    #[serde(default)]
    pub password_source: PasswordSource,

    /// The interval at which system statistics are collected and published.
    /// This determines how frequently the daemon will report system metrics.
    pub update_interval: Duration,

    /// The interval at which Home Assistant discovery messages are sent.
    /// If not specified, defaults to once per hour.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_interval: Option<Duration>,

    /// The list of drives to monitor for disk usage.
    /// Each drive configuration specifies a mount point and a name for reporting.
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

/// Configuration for a monitored drive.
#[derive(Serialize, Deserialize, Clone)]
pub struct DriveConfig {
    /// The mount point path of the drive to monitor.
    pub path: PathBuf,
    /// The name to use when reporting this drive's statistics.
    pub name: String,
}

/// Source of the MQTT password.
#[derive(Serialize, Deserialize, Clone)]
pub enum PasswordSource {
    /// Use the system keyring to store and retrieve the password.
    #[serde(rename = "keyring")]
    Keyring,

    /// Read the password from a file.
    /// The file should be readable only by the user running the daemon.
    #[serde(rename = "secret_file")]
    SecretFile(PathBuf),

    /// Use a plaintext password directly in the configuration.
    /// Note: This is less secure than other options.
    #[serde(rename = "plaintext")]
    Plaintext(String),
}

impl Default for PasswordSource {
    fn default() -> Self {
        Self::Keyring
    }
}
