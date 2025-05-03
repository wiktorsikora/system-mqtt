mod discovery;
mod home_assistant;
mod lm_sensors_impl;
mod config;

use anyhow::{bail, Context, Result};
use argh::FromArgs;
use rumqttc::{MqttOptions, Transport, AsyncClient, ConnectionError};
use std::{
    collections::{HashMap},
    path::{PathBuf},
    time::Duration,
};
use std::convert::TryFrom;
use serde_json::Value;
use sysinfo::{CpuExt, DiskExt, System, SystemExt};
use tokio::{fs, signal, time};
use tokio::task::JoinHandle;
use tokio::time::Instant;
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

                // log::set_max_level(log::LevelFilter::Info);

                while let Err(error) = application_trampoline(&config).await {
                    log::error!("Fatal error: {error:#}");
                    log::error!("Restarting in 5 seconds...");
                    time::sleep(Duration::from_secs(5)).await;
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

    let mut system = System::new_all();

    let hostname = system
        .host_name()
        .context("Could not get system hostname.")?;

    let device_id = config
        .unique_id
        .clone()
        .unwrap_or_else(|| hostname);

    let mut url = config
        .mqtt_server
        .clone();
    let client_id = format!("system-mqtt-{}", device_id);

    // add client id to the URL
    url.query_pairs_mut()
        .append_pair("client_id", &client_id);

    // eprintln!("url = {:#?}", url);

    let mut mqtt_options = MqttOptions::try_from(url)
        .context("failed to create MQTT options")?;

    // eprintln!("mqtt_options = {:#?}", mqtt_options);

    if let Some(ca_cert) = &config.ca_cert {
        let ca_cert = fs::read(ca_cert)
            .await
            .context("Failed to read CA certificate.")?;
        let transport = Transport::tls(ca_cert, None, None);
        mqtt_options.set_transport(transport);
    };

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
    let manager = battery::Manager::new().context("Failed to initialize battery monitoring.")?;

    let mut home_assistant = HomeAssistant::new(device_id, client)?;

    // Register the various sensor topics and include the details about that sensor
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
    sensors.register_sensors(&mut home_assistant).await?;

    home_assistant.set_available(true).await?;

    let mqtt_task = mqtt_loop(eventloop).await;

    let result =
        availability_trampoline(mqtt_task, &home_assistant, &mut system, config, manager, sensors).await;

    if let Err(error) = home_assistant.set_available(false).await {
        log::error!("Error while disconnecting from home assistant: {:#}", error);
    }

    result?;

    home_assistant.disconnect().await?;

    Ok(())
}

async fn mqtt_loop(
    mut eventloop: rumqttc::EventLoop
) -> JoinHandle<std::result::Result<(), ConnectionError>> {
    tokio::spawn(async move {
        loop {
            match eventloop.poll().await {
                Ok(_) => {}
                Err(e) => {
                    log::error!("Error in MQTT loop: {:#}", e);
                    break Err(e);
                }
            }
        }
    })
}

async fn availability_trampoline(
    mut mqtt_task: JoinHandle<std::result::Result<(), ConnectionError>>,
    home_assistant: &HomeAssistant,
    system: &mut System,
    config: &Config,
    manager: battery::Manager,
    mut sensors: SensorsImpl,
) -> Result<()> {
    let drive_list: HashMap<PathBuf, String> = config
        .drives
        .iter()
        .map(|drive_config| (drive_config.path.clone(), drive_config.name.clone()))
        .collect();

    system.refresh_disks();
    system.refresh_memory();
    system.refresh_cpu();

    let mut discovery_interval = tokio::time::interval_at(Instant::now(), config.discovery_interval.unwrap_or(Duration::from_secs(60 * 60)));
    let mut interval = tokio::time::interval_at(Instant::now(), config.update_interval);

    // create fused mqtt_task
    use futures_util::future::FutureExt;
    let mut mqtt_fut = mqtt_task.fuse();

    loop {
        tokio::select! {
            result = &mut mqtt_fut => {
                match result {
                    Ok(Ok(_)) => {
                        log::info!("MQTT task completed successfully, exiting.");
                        break;
                    }
                    Ok(Err(e)) => {
                        log::error!("MQTT task failed: {:#}", e);
                        return Err(e).context("MQTT task failed.");
                    }
                    Err(e) => {
                        log::error!("MQTT task failed: {:#}", e);
                        return Err(e).context("MQTT task failed.");
                    }
                }
            }
            _ = discovery_interval.tick() => {
                home_assistant.publish_discovery().await?
            }
            _ = interval.tick() => {
                system.refresh_disks();
                system.refresh_memory();
                system.refresh_cpu();

                let mut stats = HashMap::new();

                // Collect uptime.
                let uptime = system.uptime() as f32 / 60.0 / 60.0 / 24.0; // Convert from seconds to days.
                stats.insert("uptime".to_string(), Value::from(uptime));

                // Collect CPU usage.
                let cpu_usage = (system.cpus().iter().map(|cpu| cpu.cpu_usage()).sum::<f32>()) / (system.cpus().len() as f32 * 100.0);
                stats.insert("cpu".to_string(), Value::from(cpu_usage * 100.0));

                // Collect memory usage.
                let memory_percentile = (system.total_memory() - system.available_memory()) as f64 / system.total_memory() as f64;
                stats.insert("memory".to_string(), Value::from(memory_percentile.clamp(0.0, 1.0) * 100.0));

                // Collect swap usage.
                let swap_percentile = system.used_swap() as f64 / system.free_swap() as f64;
                stats.insert("swap".to_string(), Value::from(swap_percentile.clamp(0.0, 1.0) * 100.0));

                // Collect filesystem usage.
                for drive in system.disks() {
                    if let Some(drive_name) = drive_list.get(drive.mount_point()) {
                        let drive_percentile = (drive.total_space() - drive.available_space()) as f64 / drive.total_space() as f64;
                        stats.insert(drive_name.clone(), Value::from(drive_percentile.clamp(0.0, 1.0) * 100.0));
                    }
                }

                // Collect battery information.
                if let Some(battery) = manager.batteries().context("Failed to read battery info.")?.flatten().next() {
                    use battery::State;

                    let battery_state = match battery.state() {
                        State::Charging => "charging",
                        State::Discharging => "discharging",
                        State::Empty => "empty",
                        State::Full => "full",
                        _ => "unknown",
                    };
                    stats.insert("battery_state".to_string(), Value::from(battery_state));

                    let battery_full = battery.energy_full();
                    let battery_power = battery.energy();
                    let battery_level = battery_power / battery_full;

                    stats.insert("battery_level".to_string(), Value::from(battery_level.value));
                }

                // Collect lm_sensors data.
                sensors.collect_values(&mut stats).await?;

                // Serialize stats to JSON and publish.
                let json_message = serde_json::to_string(&stats).context("Failed to serialize stats to JSON.")?;
                home_assistant.publish("state", json_message).await;
            }
            _ = signal::ctrl_c() => {
                log::info!("Terminate signal has been received.");
                break;
            }
        }
    }

    Ok(())
}