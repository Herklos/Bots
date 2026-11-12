//! The `hbots` command line: set up, run, check and update the gateway.
//! `hbots serve` (and no command at all, as before) returns to `main`, which starts the gateway.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::json;
use std::{
    cmp::Ordering,
    collections::HashMap,
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
    time::Duration,
};

mod env;
mod t3;
#[cfg(test)]
mod tests;
mod update;

#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use env::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use t3::*;
#[allow(unused_imports, reason = "siblings and old paths use these names")]
pub(crate) use update::*;

const HELP: &str = "\
hbots VERSION
TAGLINE

Usage: hbots <command> [options]

  up                 Set up Bots on this machine and start it in the background.
                       --port <n>       Port for the app (default 3780)
                       --t3-url <url>   T3 Code server (default http://127.0.0.1:3773)
                       --no-service     Write the settings only; start it yourself with `hbots serve`
                       --refresh-token  Ask T3 Code for a new token, even when this one can operate the browser
  serve              Run the gateway in this terminal.
  status [--json]    Show the version, the service and the gateway.
  update [version]   Install the newest release (or <version>) and restart the service.
                       --check            Only say whether there is a newer release
                       --preview          Include preview releases
                       --yes              Don't ask before restarting
                       --allow-downgrade  Allow an older version
  rollback           Go back to the version before the last update.
  service <install|uninstall|restart|status>
  version            Print the version.
  help               Print this help.

Data, settings and installed versions live in HBOTS_HOME (default ~/.hbots).
";

/// Run a command. Returns the exit code, or None when the gateway should start (`serve`, or no command).
pub(crate) async fn run(args: &[String]) -> Option<i32> {
    if args.iter().any(|a| a == "--version" || a == "-V")
        || args.first().map(String::as_str) == Some("version")
    {
        println!("hbots {}", crate::platform::release::VERSION);
        return Some(0);
    }
    let command = args.first()?;
    if command == "serve" {
        return None;
    }
    let (flags, rest) = parse_args(&args[1..]);
    let result = match command.as_str() {
        "help" | "--help" | "-h" => {
            print!(
                "{}",
                HELP.replace("VERSION", crate::platform::release::VERSION)
                    .replace("TAGLINE", &hbots_design::tokens().brand.tagline)
            );
            Ok(())
        }
        "up" => up(&flags).await,
        "status" => status(&flags),
        "update" => update(&flags, &rest).await,
        "rollback" => rollback(),
        "service" => service_command(&rest),
        other => {
            eprintln!("hbots: Unknown command: {other}. Run `hbots help`.");
            return Some(2);
        }
    };
    match result {
        Ok(()) => Some(0),
        Err(error) => {
            eprintln!("hbots: {error:#}");
            Some(1)
        }
    }
}
