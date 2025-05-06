use anyhow::{Context, Result};
use rumqttc::{MqttOptions, Transport, AsyncClient, ConnectionError, Event, Packet};
use std::convert::TryFrom;
use tokio::fs;
use tokio::task::JoinHandle;
use crate::config::{Config, PasswordSource};
use crate::password::KEYRING_SERVICE_NAME;

/// Setup MQTT client with the given configuration
pub async fn setup_mqtt_client(config: &Config, device_id: &str) -> Result<(AsyncClient, rumqttc::EventLoop)> {
    let mut url = config.mqtt_server.clone();
    let client_id = format!("system-mqtt-{}", device_id);

    // add client id to the URL
    url.query_pairs_mut()
        .append_pair("client_id", &client_id);

    let mut mqtt_options = MqttOptions::try_from(url)
        .context("failed to create MQTT options")?;


    if let Some(ca_cert) = &config.ca_cert {
        let ca_cert = fs::read(ca_cert)
            .await
            .context("Failed to read CA certificate.")?;
        let transport = Transport::tls(ca_cert, None, None);
        mqtt_options.set_transport(transport);
    };

    match mqtt_options.transport() {
        Transport::Tcp => {
            log::info!("Connecting to MQTT server using insecure connection.");
        }
        Transport::Tls(_) => {
            log::info!("Connecting to MQTT server with using secure connection.");
        }
        Transport::Unix => {
            log::info!("Connecting to MQTT server using Unix socket.");
        }
    }


    // Set credentials if provided
    if let Some(username) = &config.username {
        let password = match &config.password_source {
            PasswordSource::Keyring => {
                log::info!("Using system keyring for MQTT password source.");
                let keyring = keyring::Entry::new(KEYRING_SERVICE_NAME, username)
                    .context("Failed to find password entry in keyring.")?;
                keyring
                    .get_password()
                    .context("Failed to get password from keyring. If you have not yet set the password, run `system-mqtt set-password`.")?
            }
            PasswordSource::SecretFile(file_path) => {
                log::info!("Using hidden file for MQTT password source.");
                let pass: String = fs::read_to_string(file_path)
                    .await
                    .context("Failed to read password file.")?;
                pass.trim_end().to_string()
            }
            PasswordSource::Plaintext(passwd) => {
                log::info!("Using plaintext password for MQTT password source.");
                passwd.clone()
            }
        };

        mqtt_options.set_credentials(username.clone(), password);
    }

    let (client, eventloop) = AsyncClient::new(mqtt_options, 10);
    Ok((client, eventloop))
}

/// Run the MQTT event loop in a separate task
pub async fn mqtt_loop(
    mut eventloop: rumqttc::EventLoop
) -> JoinHandle<std::result::Result<(), ConnectionError>> {
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    log::info!("Connected to MQTT broker.");
                }
                Err(e) => {
                    log::error!("Error in MQTT loop: {:#}", e);
                    break Err(e);
                }
                _ => {}
            }
        }
    })
}