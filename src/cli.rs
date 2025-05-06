use argh::FromArgs;
use std::path::PathBuf;

/// Push system statistics to a mqtt server.
#[derive(FromArgs)]
pub struct Arguments {
    /// the configuration file we are to use.
    #[argh(option, default = "PathBuf::from(\"/etc/system-mqtt.yaml\")")]
    pub config_file: PathBuf,

    #[argh(subcommand)]
    pub command: SubCommand,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
pub enum SubCommand {
    Run(RunArguments),
    SetPassword(SetPasswordArguments),
}

/// Run the daemon.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "run")]
pub struct RunArguments {
    /// log to stderr instead of systemd's journal.
    #[argh(switch)]
    pub log_to_stderr: bool,
}

/// Set the password used to log into the mqtt client.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "set-password")]
pub struct SetPasswordArguments {}