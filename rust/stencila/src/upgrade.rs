use std::{fs, thread};

use common::{
    chrono::{DateTime, Duration, Utc},
    eyre::{bail, eyre, Result},
    tokio,
};

use crate::config::CONFIG;

/// Upgrade the application
///
/// Checks for a higher version on [GitHub releases](https://github.com/stencila/stencila/releases)
/// and downloads the binary for the current platform if one is found.
///
/// Optionally checks for new versions and, upgrades if necessary, all installed plugins.
/// See `plugins::upgrade_list` to only upgrade certain plugins.
///
/// # Arguments
///
/// - `current_version`: The current version (used mainly for testing)
/// - `wanted_version`: The version that is wanted (other than latest)
/// - `include_plugins`: Whether to upgrade installed plugins to their latest version
/// - `confirm`: Prompt the user to confirm an upgrade
/// - `verbose`: Print information on the upgrade process
pub async fn upgrade(
    current_version: Option<String>,
    wanted_version: Option<String>,
    _include_plugins: bool,
    confirm: bool,
    verbose: bool,
) -> Result<()> {
    let mut builder = self_update::backends::github::Update::configure();
    builder
        .repo_owner("stencila")
        .repo_name("stencila")
        .bin_name("stencila")
        .current_version(
            current_version
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
                .as_str(),
        )
        .no_confirm(!confirm)
        .show_output(verbose)
        .show_download_progress(verbose);

    if let Some(version) = wanted_version {
        builder.target_version_tag(format!("v{}", version).as_str());
    }

    // The actual upgrade is run in a separate thread because `self_update`
    // creates a new `tokio` runtime (which can not be nested within our main `tokio` runtime).
    thread::spawn(move || -> Result<()> {
        // Fail silently unless `verbose` is true.
        match builder.build()?.update() {
            Ok(_status) => Ok(()),
            Err(error) => {
                let message = error.to_string();
                if !message.contains("Update aborted") && verbose {
                    bail!("Error attempting to upgrade: {}", message)
                } else {
                    Ok(())
                }
            }
        }
    })
    .join()
    .map_err(|_| eyre!("Error joining thread"))??;

    #[cfg(feature = "plugins")]
    if include_plugins {
        Plugin::upgrade_all(&mut *PLUGINS.lock().await).await?;
    }

    Ok(())
}

const UPGRADE_FILE: &str = "cli-upgrade.txt";

/// Do a upgrade check automatically if not within the configured interval
/// since the last check.
///
/// Runs in a separate thread so that is does not slow down the
/// command currently being run by the user.
///
/// Note that the in-memory state of application and plugins is unchanged after this call
/// A restart is required to upload both the new version and plugin versions.
///
/// Because this function use values form the config file, requires
/// that `feature = "config"` is enabled.

pub async fn upgrade_auto() -> tokio::task::JoinHandle<Result<()>> {
    let config::UpgradeConfig {
        auto,
        confirm,
        plugins: include_plugins,
        ..
    } = CONFIG.lock().await.upgrade.clone();

    tokio::spawn(async move {
        // Go no further if auto upgrade is not enabled
        if auto == "off" {
            return Ok(());
        }

        // Check if within the time since the last check was done
        let upgrade_file = crate::config::dir(true)?.join(UPGRADE_FILE);
        let last = match fs::read_to_string(upgrade_file.clone()) {
            Ok(date) => DateTime::parse_from_rfc3339(date.as_str())?.with_timezone(&Utc),
            Err(_) => Utc::now(),
        };
        let duration = Duration::from_std(humantime::parse_duration(auto.as_str())?)?;
        if Utc::now() < last + duration {
            return Ok(());
        }

        // Attempt an upgrade
        upgrade(None, None, include_plugins, confirm, false).await?;

        // Record the time of the upgrade check, so another check
        // is not made within the `auto`.
        let now = Utc::now();
        fs::write(upgrade_file, now.to_rfc3339())?;

        Ok(())
    })
}

pub mod config {
    use common::{
        defaults::Defaults,
        serde::{Deserialize, Serialize},
    };
    use schemars::JsonSchema;
    use validator::{Validate, ValidationError};

    /// Upgrade
    ///
    /// Configuration settings used when upgrading the application (and optionally plugins)
    /// automatically, in the background. These settings are NOT used as defaults when
    /// using the CLI `upgrade` command directly.
    #[derive(Debug, Defaults, PartialEq, Clone, JsonSchema, Deserialize, Serialize, Validate)]
    #[serde(default, crate = "common::serde")]
    #[schemars(deny_unknown_fields)]
    pub struct UpgradeConfig {
        /// Plugins should also be upgraded to latest version
        #[def = "true"]
        pub plugins: bool,

        /// Prompt to confirm an upgrade
        #[def = "true"]
        pub confirm: bool,

        /// Show information on the upgrade process
        #[def = "false"]
        pub verbose: bool,

        /// The interval between automatic upgrade checks (defaults to "1 day").
        /// Only used when for configuration. Set to "off" for no automatic checks.
        #[def = "\"1 day\".to_string()"]
        #[validate(
            length(min = 2),
            custom(function = "validate_auto", message = "Not a valid duration")
        )]
        pub auto: String,
    }

    /// Validate `auto` (a valid duration or "off")
    pub fn validate_auto(value: &str) -> Result<(), ValidationError> {
        if value == "off" {
            return Ok(());
        }
        if humantime::parse_duration(value).is_err() {
            return Err(ValidationError::new("invalid_duration_string"));
        }
        Ok(())
    }
}

#[cfg(feature = "cli")]
pub mod commands {
    use cli_utils::{
        clap::{self, Parser},
        result, Result, Run,
    };
    use common::async_trait::async_trait;

    use super::*;

    /// Upgrade to the latest (or other) version
    #[derive(Parser)]
    pub struct Command {
        /// Version to upgrade (or downgrade) to (defaults to the latest)
        #[clap(short, long)]
        pub to: Option<String>,

        /// Plugins should also be upgraded to their latest version
        #[clap(short, long)]
        pub plugins: bool,

        /// The user should be asked to confirm an upgrade
        #[clap(short, long)]
        pub confirm: bool,

        /// Print information on the upgrade process
        #[clap(short, long)]
        pub verbose: bool,
    }

    /// Run the upgrade.
    ///
    /// Note that the in-memory state of application and plugins is unchanged after this call
    /// (e.g. if called in interactive mode). A restart is required to upload both the new
    /// version and plugin versions.
    #[async_trait]
    impl Run for Command {
        async fn run(&self) -> Result {
            upgrade(
                None,
                self.to.clone(),
                self.plugins,
                self.confirm,
                self.verbose,
            )
            .await?;
            result::nothing()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note that these tests are a little funky in that they make requests to the
    // GitHub API. They currently also have few assertions.
    // They use an artificially high `current_version` to avoid any binaries
    // from being downloaded.
    // Currently ignoring them because they involve network requests. Mocking _may_ be
    // useful here.

    #[ignore]
    #[tokio::test]
    async fn test_upgrade() -> Result<()> {
        upgrade(Some("100.0.0".to_string()), None, false, false, false).await
    }

    #[ignore]
    #[tokio::test]
    async fn test_upgrade_auto() -> Result<()> {
        let handle = upgrade_auto().await;
        handle.await?
    }

    #[ignore]
    #[tokio::test]
    async fn test_cli() -> Result<()> {
        use super::commands::Command;
        use cli_utils::Run;

        Command {
            to: None,
            plugins: false,
            confirm: false,
            verbose: false,
        }
        .run()
        .await?;

        Ok(())
    }

    #[test]
    fn test_validate_auto() {
        assert_eq!(config::validate_auto("off"), Ok(()));
        assert_eq!(config::validate_auto("1 day"), Ok(()));
        assert_eq!(config::validate_auto("2 weeks 3 days 1 hr"), Ok(()));

        assert_eq!(
            config::validate_auto("foo").unwrap_err().to_string(),
            "Validation error: invalid_duration_string [{}]".to_string()
        );
    }
}
