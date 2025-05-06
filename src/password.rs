use anyhow::{Context, Result, bail};
use crate::config::Config;

pub const KEYRING_SERVICE_NAME: &str = "system-mqtt";

/// Set the password for the MQTT server in the system keyring
pub async fn set_password(config: Config) -> Result<()> {
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