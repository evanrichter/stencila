use std::path::Path;

use schemars::JsonSchema;
use validator::Validate;

use common::{
    chrono::{DateTime, Utc},
    eyre::Result,
    serde::{Deserialize, Serialize},
    serde_json,
    strum::{Display, EnumString, EnumVariantNames},
    tracing::{self, Event},
};
use events::publish;

/// Logging level
#[derive(
    Debug,
    Display,
    Clone,
    Copy,
    PartialEq,
    PartialOrd,
    JsonSchema,
    Deserialize,
    Serialize,
    EnumString,
    EnumVariantNames,
)]
#[serde(rename_all = "lowercase", crate = "common::serde")]
#[strum(serialize_all = "lowercase", crate = "common::strum")]
pub enum LoggingLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Never,
}

/// Create a `LoggingLevel` from a `tracing::Level`
impl From<&tracing::Level> for LoggingLevel {
    fn from(level: &tracing::Level) -> Self {
        match *level {
            tracing::Level::TRACE => Self::Trace,
            tracing::Level::DEBUG => Self::Debug,
            tracing::Level::INFO => Self::Info,
            tracing::Level::WARN => Self::Warn,
            tracing::Level::ERROR => Self::Error,
        }
    }
}

/// Logging format
#[derive(
    Debug, PartialEq, Clone, Copy, JsonSchema, Deserialize, Serialize, EnumString, EnumVariantNames,
)]
#[serde(rename_all = "lowercase", crate = "common::serde")]
#[strum(serialize_all = "lowercase", crate = "common::strum")]
pub enum LoggingFormat {
    Simple,
    Detail,
    Json,
}

pub mod config {
    use std::{
        fs,
        {env, path::PathBuf},
    };

    use common::defaults::Defaults;

    use super::*;

    /// Get the directory where logs are stored
    pub fn dir(ensure: bool) -> Result<PathBuf> {
        let config = crate::config::dir(false)?;
        let dir = match env::consts::OS {
            "macos" | "windows" => config.join("Logs"),
            _ => config.join("logs"),
        };
        if ensure {
            fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    /// Logging to standard error stream
    ///
    /// Configuration settings for log entries printed to stderr when using the CLI
    #[derive(
        Debug, Defaults, PartialEq, Clone, Copy, JsonSchema, Deserialize, Serialize, Validate,
    )]
    #[serde(default, crate = "common::serde")]
    #[schemars(deny_unknown_fields)]
    pub struct LoggingStdErrConfig {
        /// The maximum log level to emit
        #[def = "LoggingLevel::Info"]
        pub level: LoggingLevel,

        /// The format for the logs entries
        #[def = "LoggingFormat::Simple"]
        pub format: LoggingFormat,
    }

    /// Logging to desktop notifications
    ///
    /// Configuration settings for log entries shown to the user in the desktop
    #[derive(
        Debug, Defaults, PartialEq, Clone, Copy, JsonSchema, Deserialize, Serialize, Validate,
    )]
    #[serde(default, crate = "common::serde")]
    #[schemars(deny_unknown_fields)]
    pub struct LoggingDesktopConfig {
        /// The maximum log level to emit
        #[def = "LoggingLevel::Info"]
        pub level: LoggingLevel,
    }

    /// Logging to file
    ///
    /// Configuration settings for logs entries written to file
    #[derive(Debug, Defaults, PartialEq, Clone, JsonSchema, Deserialize, Serialize, Validate)]
    #[serde(default, crate = "common::serde")]
    #[schemars(deny_unknown_fields)]
    pub struct LoggingFileConfig {
        /// The path of the log file
        #[def = "default_file_path()"]
        pub path: String,

        /// The maximum log level to emit
        #[def = "LoggingLevel::Info"]
        pub level: LoggingLevel,
    }

    /// Get the default value for `logging.file.path`
    pub fn default_file_path() -> String {
        dir(true)
            .expect("Unable to get logs directory")
            .join("log.json")
            .into_os_string()
            .into_string()
            .expect("Unable to convert path to string")
    }

    /// Logging
    ///
    /// Configuration settings for logging
    #[derive(Debug, Default, PartialEq, Clone, JsonSchema, Deserialize, Serialize, Validate)]
    #[serde(default, crate = "common::serde")]
    #[schemars(deny_unknown_fields)]
    pub struct LoggingConfig {
        pub stderr: LoggingStdErrConfig,
        pub desktop: LoggingDesktopConfig,
        pub file: LoggingFileConfig,
    }
}

/// Create a preliminary logging subscriber.
///
/// This can be necessary to ensure that any log events that get emitted during
/// initialization are displayed to the user.
pub fn prelim() -> tracing::subscriber::DefaultGuard {
    let subscriber = tracing_subscriber::fmt()
        .pretty()
        .with_max_level(tracing::Level::INFO)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_default(subscriber)
}

/// Custom tracing_subscriber layer that prints events to stderr filtered
/// by level for the "plain" format. Other formats are handled by `tracing_subscriber`
/// formatters (see below).
struct StderrPlainLayer {
    level: LoggingLevel,
}

#[derive(Default)]
struct StderrPlainVisitor {
    message: String,
}

impl tracing::field::Visit for StderrPlainVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
}

impl<S: tracing::subscriber::Subscriber> tracing_subscriber::layer::Layer<S> for StderrPlainLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let level = LoggingLevel::from(event.metadata().level());
        if level >= self.level {
            let level_name = level.to_string().to_uppercase();

            #[cfg(feature = "cli-pretty")]
            let level_name = {
                use cli_utils::ansi_term::Color::{Blue, Green, Purple, Red, White, Yellow};
                match level {
                    LoggingLevel::Trace => Purple,
                    LoggingLevel::Debug => Blue,
                    LoggingLevel::Info => Green,
                    LoggingLevel::Warn => Yellow,
                    LoggingLevel::Error => Red,
                    _ => White,
                }
                .bold()
                .paint(format!("{:5}", level_name))
            };

            let mut visitor = StderrPlainVisitor::default();
            event.record(&mut visitor);
            eprintln!("{} {}", level_name, visitor.message)
        }
    }
}

/// Custom tracing_subscriber layer that prints events to stderr in a custom JSON structure
/// that is consistent with JSON log and error entries used elsewhere in Stencila
///
/// See https://burgers.io/custom-logging-in-rust-using-tracing
struct StderrJsonLayer {
    level: LoggingLevel,
}

#[derive(Serialize)]
#[serde(crate = "common::serde")]
struct StderrJsonVisitor {
    time: DateTime<Utc>,
    level: LoggingLevel,
    message: String,

    #[serde(skip_serializing)]
    min_level: LoggingLevel,

    #[serde(skip_serializing_if = "Option::is_none")]
    module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u64>,
}

impl StderrJsonVisitor {
    fn new(level: LoggingLevel, min_level: LoggingLevel) -> Self {
        Self {
            time: Utc::now(),
            level,
            message: "".to_string(),
            min_level,
            module: None,
            file: None,
            line: None,
        }
    }
}

impl tracing::field::Visit for StderrJsonVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "log.module_path" {
            self.module = Some(value.to_string());
        } else if self.min_level >= LoggingLevel::Debug && field.name() == "log.file" {
            self.file = Some(value.to_string());
        }
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        if self.min_level >= LoggingLevel::Debug && field.name() == "log.line" {
            self.line = Some(value);
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
}

impl<S: tracing::subscriber::Subscriber> tracing_subscriber::layer::Layer<S> for StderrJsonLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let level = LoggingLevel::from(event.metadata().level());
        if level >= self.level {
            let mut visitor = StderrJsonVisitor::new(level, self.level);
            event.record(&mut visitor);
            if let Ok(json) = serde_json::to_string(&visitor) {
                eprintln!("{}", json)
            }
        }
    }
}

/// Custom tracing_subscriber layer that publishes events
/// under the pubsub "logging" topic as a JSON value.
struct PubSubLayer {
    level: LoggingLevel,
}

impl<S: tracing::subscriber::Subscriber> tracing_subscriber::layer::Layer<S> for PubSubLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        use tracing_serde::AsSerde;
        if LoggingLevel::from(event.metadata().level()) >= self.level {
            publish("logging", &event.as_serde())
        }
    }
}

/// Initialize logging
///
/// This initializes a logging subscriber based on configuration and
/// context (e.g. stderr should not be written to if the context
/// is the desktop application).
///
/// # Arguments
///
/// - `stderr`: should stderr logging be enabled
/// - `pubsub`: should pubsub logging be enabled (for desktop notifications)
/// - `file`: should file logging be enabled
/// - `config`: the logging configuration
pub fn init(
    stderr: bool,
    pubsub: bool,
    file: bool,
    config: &config::LoggingConfig,
) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter};

    // Stderr logging layer
    let stderr_level = if stderr {
        config.stderr.level
    } else {
        LoggingLevel::Never
    };

    // Pubsub logging layer (used for desktop notifications)
    let pubsub_level = if pubsub {
        config.desktop.level
    } else {
        LoggingLevel::Never
    };
    let pubsub_layer = PubSubLayer {
        level: pubsub_level,
    };

    // File logging layer
    let file_level = if file {
        config.file.level
    } else {
        LoggingLevel::Never
    };
    let (file_writer, file_guard) = if file_level != LoggingLevel::Never {
        let path = Path::new(&config.file.path);
        let file_appender =
            tracing_appender::rolling::daily(&path.parent().unwrap(), &path.file_name().unwrap());
        tracing_appender::non_blocking(file_appender)
    } else {
        tracing_appender::non_blocking(std::io::sink())
    };
    let file_layer = fmt::Layer::new().json().with_writer(file_writer);

    // Error reporting layer (necessary for using `eyre` crate)
    let error_layer = ErrorLayer::default();

    // tracing_subscriber does not currently allow for different layers to have different
    // levels so work out the minimum level and filter by that in the root subscriber.
    let mut min_level = LoggingLevel::Never;
    if stderr_level < min_level {
        min_level = stderr_level
    }
    if pubsub_level < min_level {
        min_level = pubsub_level
    }
    if file_level < min_level {
        min_level = file_level
    }

    // Filter out debug log entries from some crates to avoid excessive noise.
    // We may want to show entries from other crates during development
    // so we may add another flag for this in the future.
    // e.g. `--log-scope=stencila` vs `--log-scope=all`.
    let directives = format!(
        "{},async_io=info,async_std=info,chromiumoxide=info,html5ever=info,hyper=info,mio=info,polling=info,reqwest=info,rustyline=info,tokio_tungstenite=info,tungstenite=info,want=info,warp=info",
        min_level
    );

    let registry = tracing_subscriber::registry()
        .with(EnvFilter::new(directives))
        .with(pubsub_layer)
        .with(file_layer)
        .with(error_layer);

    if config.stderr.format == LoggingFormat::Detail {
        registry.with(fmt::Layer::new().pretty()).init();
    } else if config.stderr.format == LoggingFormat::Json {
        registry
            .with(StderrJsonLayer {
                level: stderr_level,
            })
            .init();
    } else {
        registry
            .with(StderrPlainLayer {
                level: stderr_level,
            })
            .init();
    }

    Ok(file_guard)
}

/// Generate some test tracing events.
///
/// Can be used for testing that events are propagated
/// to subscribers as expected.
#[tracing::instrument]
pub fn test_events() {
    tracing::trace!("A trace event");
    tracing::debug!("A debug event");
    tracing::info!("An info event");
    tracing::warn!("A warn event");
    tracing::error!("An error event");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::eq_op)]
    fn test_logging_level_ordering() -> Result<()> {
        assert!(LoggingLevel::Debug > LoggingLevel::Trace);
        assert!(LoggingLevel::Debug >= LoggingLevel::Debug);
        assert!(LoggingLevel::Debug == LoggingLevel::Debug);
        assert!(LoggingLevel::Trace <= LoggingLevel::Debug);
        assert!(LoggingLevel::Trace < LoggingLevel::Debug);

        assert!(LoggingLevel::Info > LoggingLevel::Debug);
        assert!(LoggingLevel::Warn > LoggingLevel::Info);
        assert!(LoggingLevel::Error > LoggingLevel::Warn);
        Ok(())
    }
}
