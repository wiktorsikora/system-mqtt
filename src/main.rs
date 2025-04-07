mod discovery;
mod home_assistant;
mod lm_sensors_impl;
mod config;

use anyhow::{bail, Context, Result};
use argh::FromArgs;
use mqtt_async_client::client::{Client as MqttClient};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    os::unix::prelude::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};
use sysinfo::{CpuExt, DiskExt, System, SystemExt};
use tokio::{fs, signal, time};
use url::Url;
use crate::config::{load_config, Config, PasswordSource};
use crate::home_assistant::HomeAssistant;
use crate::lm_sensors_impl::SensorsImpl;

const KEYRING_SERVICE_NAME: &str = "system-mqtt";

#[derive(FromArgs)]
/// Push system statistics to an mqtt server.
struct Arguments {
    /// the configuration file we are to use.
    #[argh(option, default = "PathBuf::from(\"/etc/system-mqtt.yaml\")")]
    config_file: PathBuf,

    #[argh(subcommand)]
    command: SubCommand,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum SubCommand {
    Run(RunArguments),
    SetPassword(SetPasswordArguments),
}

#[derive(FromArgs, PartialEq, Debug)]
/// Run the daemon.
#[argh(subcommand, name = "run")]
struct RunArguments {
    /// log to stderr instead of systemd's journal.
    #[argh(switch)]
    log_to_stderr: bool,
}

#[derive(FromArgs, PartialEq, Debug)]
/// Set the password used to log into the mqtt client.
#[argh(subcommand, name = "set-password")]
struct SetPasswordArguments {}

#[tokio::main]
async fn main() {
    let arguments: Arguments = argh::from_env();

    match load_config(&arguments.config_file).await {
        Ok(config) => match arguments.command {
            SubCommand::Run(arguments) => {
                if arguments.log_to_stderr {
                    simple_logger::SimpleLogger::new()
                        .env()
                        .init()
                        .expect("Failed to setup log.");
                } else {
                    systemd_journal_logger::init().expect("Failed to setup log.");
                }

                log::set_max_level(log::LevelFilter::Info);

                while let Err(error) = application_trampoline(&config).await {
                    log::error!("Fatal error: {}", error);
                }
            }
            SubCommand::SetPassword(_arguments) => {
                if let Err(error) = set_password(config).await {
                    eprintln!("Fatal error: {}", error);
                }
            }
        },
        Err(error) => {
            eprintln!("Failed to load config file: {}", error);
        }
    }
}


async fn set_password(config: Config) -> Result<()> {
    if let Some(username) = config.username {
        let password = rpassword::prompt_password("Password: ")
            .context("Failed to read password from TTY.")?;

        let keyring = keyring::Entry::new(KEYRING_SERVICE_NAME, &username)
            .context("Failed to find password entry in keyring.")?;
        keyring.set_password(&password).context("Keyring error.")?;

        Ok(())
    } else {
        bail!("You must set the username for login with the mqtt server before you can set the user's password")
    }
}

async fn application_trampoline(config: &Config) -> Result<()> {
    log::info!("Application start.");

    let mut client_builder = MqttClient::builder();
    client_builder.set_url_string(config.mqtt_server.as_str())?;

    // If credentials are provided, use them.
    if let Some(username) = &config.username {
        // TODO make TLS mandatory when using a password.

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
                let metadata = file_path
                    .metadata()
                    .context("Failed to get password file metadata.")?;

                let pass: String = fs::read_to_string(file_path)
                    .await
                    .context("Failed to read password file.")?;
                pass.as_str().trim_end().to_string()

                // It's not even an encrypted file, so we need to keep the permission settings pretty tight.
                // The only time I can really enforce that is when reading the password.
                // if metadata.mode() & 0o777 == 0o600 {
                //     if metadata.uid() == users::get_current_uid() {
                //         if metadata.gid() == users::get_current_gid() {
                //             let pass: String = fs::read_to_string(file_path)
                //                 .await
                //                 .context("Failed to read password file.")?;
                //             pass.as_str().trim_end().to_string()
                //         } else {
                //             bail!("Password file must be owned by the current group.");
                //         }
                //     } else {
                //         bail!("Password file must be owned by the current user.");
                //     }
                // } else {
                //     bail!("Permission bits for password file must be set to 0o600 (only owner can read and write)");
                // }
            }
        };

        client_builder.set_username(Some(username.into()));
        client_builder.set_password(Some(password.as_bytes().to_vec()));
    }

    let mut client = client_builder.build()?;
    client
        .connect()
        .await
        .context("Failed to connect to MQTT server.")?;

    let manager = battery::Manager::new().context("Failed to initialize battery monitoring.")?;

    let mut system = System::new_all();

    let hostname = system
        .host_name()
        .context("Could not get system hostname.")?;

    let device_id = config
        .unique_id
        .clone()
        .unwrap_or_else(|| hostname);

    let mut home_assistant = HomeAssistant::new(device_id, client)?;

    // Register the various sensor topics and include the details about that sensor

    //    TODO - create a new register_topic to register binary_sensor so we can make availability a real binary sensor. In the
    //    meantime, create it as a normal analog sensor with two values, and a template can be used to make it a binary.

    home_assistant
        .register_entity(
            "sensor",
            None,
            None,
            "available",
            None,
            Some("mdi:check-network-outline"),
        )
        .await
        .context("Failed to register availability topic.")?;
    home_assistant
        .register_entity(
            "sensor",
            None,
            None,
            "uptime",
            Some("days"),
            Some("mdi:timer-sand"),
        )
        .await
        .context("Failed to register uptime topic.")?;
    home_assistant
        .register_entity(
            "sensor",
            None,
            Some("measurement"),
            "cpu",
            Some("%"),
            Some("mdi:gauge"),
        )
        .await
        .context("Failed to register CPU usage topic.")?;
    home_assistant
        .register_entity(
            "sensor",
            None,
            Some("measurement"),
            "memory",
            Some("%"),
            Some("mdi:gauge"),
        )
        .await
        .context("Failed to register memory usage topic.")?;
    home_assistant
        .register_entity(
            "sensor",
            None,
            Some("measurement"),
            "swap",
            Some("%"),
            Some("mdi:gauge"),
        )
        .await
        .context("Failed to register swap usage topic.")?;
    home_assistant
        .register_entity(
            "sensor",
            Some("battery"),
            Some("measurement"),
            "battery_level",
            Some("%"),
            Some("mdi:battery"),
        )
        .await
        .context("Failed to register battery level topic.")?;
    home_assistant
        .register_entity(
            "sensor",
            None,
            None,
            "battery_state",
            None,
            Some("mdi:battery"),
        )
        .await
        .context("Failed to register battery state topic.")?;

    // Register the sensors for filesystems
    for drive in &config.drives {
        home_assistant
            .register_entity(
                "sensor",
                None,
                Some("total"),
                &drive.name,
                Some("%"),
                Some("mdi:folder"),
            )
            .await
            .context("Failed to register a filesystem topic.")?;
    }

    let mut sensors = SensorsImpl::new()?;

    // Register the sensors for lm_sensors
    sensors.register_sensors(&mut home_assistant).await?;


    home_assistant.set_available(true).await?;

    let result =
        availability_trampoline(&home_assistant, &mut system, config, manager, sensors).await;

    if let Err(error) = home_assistant.set_available(false).await {
        // I don't want this error hiding whatever happened in the main loop.
        log::error!("Error while disconnecting from home assistant: {:?}", error);
    }

    result?;

    home_assistant.disconnect().await?;

    Ok(())
}

async fn availability_trampoline(
    home_assistant: &HomeAssistant,
    system: &mut System,
    config: &Config,
    manager: battery::Manager,
    mut sensors: SensorsImpl, // Added sensors parameter
) -> Result<()> {
    let drive_list: HashMap<PathBuf, String> = config
        .drives
        .iter()
        .map(|drive_config| (drive_config.path.clone(), drive_config.name.clone()))
        .collect();

    system.refresh_disks();
    system.refresh_memory();
    system.refresh_cpu();

    loop {
        tokio::select! {
            _ = time::sleep(config.update_interval) => {
                system.refresh_disks();
                system.refresh_memory();
                system.refresh_cpu();

                // Report uptime.
                let uptime = system.uptime() as f32 / 60.0 / 60.0 / 24.0; // Convert from seconds to days.
                home_assistant.publish("uptime", format!("{}", uptime)).await;

                // Report CPU usage.
                let cpu_usage = (system.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>()) / (system.cpus().len() as f32 * 100.0);
                home_assistant.publish("cpu", (cpu_usage * 100.0).to_string()).await;

                // Report memory usage.
                let memory_percentile = (system.total_memory() - system.available_memory()) as f64 / system.total_memory() as f64;
                home_assistant.publish("memory", (memory_percentile.clamp(0.0, 1.0)* 100.0).to_string()).await;

                // Report swap usage.
                let swap_percentile = system.used_swap() as f64 / system.free_swap() as f64;
                home_assistant.publish("swap", (swap_percentile.clamp(0.0, 1.0) * 100.0).to_string()).await;

                // Report filesystem usage.
                for drive in system.disks() {
                    if let Some(drive_name) = drive_list.get(drive.mount_point()) {
                        let drive_percentile = (drive.total_space() - drive.available_space()) as f64 / drive.total_space() as f64;

                        home_assistant.publish(drive_name, (drive_percentile.clamp(0.0, 1.0) * 100.0).to_string()).await;
                    }
                }

                // TODO we should probably combine the battery charges, but for now we're just going to use the first detected battery.
                if let Some(battery) = manager.batteries().context("Failed to read battery info.")?.flatten().next() {
                    use battery::State;

                    let battery_state = match battery.state() {
                        State::Charging => "charging",
                        State::Discharging => "discharging",
                        State::Empty => "empty",
                        State::Full => "full",
                        _ => "unknown",
                    };

                    home_assistant.publish("battery_state", battery_state.to_string()).await;

                    let battery_full = battery.energy_full();
                    let battery_power = battery.energy();
                    let battery_level = battery_power / battery_full;

                    home_assistant.publish("battery_level", format!("{:03}", battery_level.value)).await;
                }

                sensors.publish_values(&home_assistant).await?
            }
            _ = signal::ctrl_c() => {
                log::info!("Terminate signal has been received.");
                break;
            }
        }
    }

    Ok(())
}
