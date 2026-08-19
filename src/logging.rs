//! Logging configuration for dependency-injector
//!
//! This module provides easy setup for structured logging with support for
//! both JSON (production) and pretty (development) output formats.
//!
//! # Features
//!
//! - `logging` - Enable debug logging (default)
//! - `logging-json` - Use JSON structured output (recommended for production)
//! - `logging-pretty` - Use colorful pretty output (recommended for development)
//!
//! # Example
//!
//! ```rust,ignore
//! use dependency_injector::logging;
//!
//! // Initialize with default settings (JSON if logging-json, pretty if logging-pretty)
//! logging::init();
//!
//! // Or initialize with specific format
//! logging::init_json();
//! logging::init_pretty();
//!
//! // Or use builder for custom configuration
//! logging::builder()
//!     .with_level(tracing::Level::DEBUG)
//!     .with_target("dependency_injector")
//!     .json()
//!     .init();
//! ```

#[cfg(feature = "logging")]
use tracing::Level;

/// Logging format configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    /// JSON structured logging (production default)
    #[default]
    Json,
    /// Pretty colorful output (development)
    Pretty,
    /// Compact single-line output
    Compact,
}

/// Builder for logging configuration
#[cfg(feature = "logging")]
#[derive(Debug, Clone)]
pub struct LoggingBuilder {
    level: Level,
    format: LogFormat,
    target: Option<&'static str>,
    with_file: bool,
    with_line_number: bool,
    with_thread_ids: bool,
    with_thread_names: bool,
}

#[cfg(feature = "logging")]
impl Default for LoggingBuilder {
    fn default() -> Self {
        Self {
            level: Level::DEBUG,
            format: LogFormat::Json,
            target: None,
            with_file: false,
            with_line_number: false,
            with_thread_ids: false,
            with_thread_names: false,
        }
    }
}

#[cfg(feature = "logging")]
impl LoggingBuilder {
    /// Create a new logging builder with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the minimum log level
    pub fn with_level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    /// Set log level to TRACE (most verbose)
    pub fn trace(mut self) -> Self {
        self.level = Level::TRACE;
        self
    }

    /// Set log level to DEBUG
    pub fn debug(mut self) -> Self {
        self.level = Level::DEBUG;
        self
    }

    /// Set log level to INFO
    pub fn info(mut self) -> Self {
        self.level = Level::INFO;
        self
    }

    /// Set log level to WARN
    pub fn warn(mut self) -> Self {
        self.level = Level::WARN;
        self
    }

    /// Set log level to ERROR (least verbose)
    pub fn error(mut self) -> Self {
        self.level = Level::ERROR;
        self
    }

    /// Filter to only show logs from a specific target
    pub fn with_target_filter(mut self, target: &'static str) -> Self {
        self.target = Some(target);
        self
    }

    /// Only show dependency-injector logs
    pub fn di_only(self) -> Self {
        self.with_target_filter("dependency_injector")
    }

    /// Include file names in log output
    pub fn with_file(mut self) -> Self {
        self.with_file = true;
        self
    }

    /// Include line numbers in log output
    pub fn with_line_number(mut self) -> Self {
        self.with_line_number = true;
        self
    }

    /// Include thread IDs in log output
    pub fn with_thread_ids(mut self) -> Self {
        self.with_thread_ids = true;
        self
    }

    /// Include thread names in log output
    pub fn with_thread_names(mut self) -> Self {
        self.with_thread_names = true;
        self
    }

    /// Use JSON structured logging format
    pub fn json(mut self) -> Self {
        self.format = LogFormat::Json;
        self
    }

    /// Use pretty colorful logging format
    pub fn pretty(mut self) -> Self {
        self.format = LogFormat::Pretty;
        self
    }

    /// Use compact single-line logging format
    pub fn compact(mut self) -> Self {
        self.format = LogFormat::Compact;
        self
    }

    /// Initialize the logging subscriber with the configured settings
    ///
    /// Requires either `logging-json` or `logging-pretty` feature to be enabled.
    #[cfg(any(feature = "logging-json", feature = "logging-pretty"))]
    pub fn init(self) {
        use tracing_subscriber::{EnvFilter, fmt, prelude::*};

        let filter = if let Some(target) = self.target {
            EnvFilter::new(format!("{}={}", target, self.level))
        } else {
            EnvFilter::new(self.level.to_string())
        };

        match self.format {
            LogFormat::Json => {
                #[cfg(feature = "logging-json")]
                {
                    let subscriber = fmt::layer()
                        .json()
                        .with_file(self.with_file)
                        .with_line_number(self.with_line_number)
                        .with_thread_ids(self.with_thread_ids)
                        .with_thread_names(self.with_thread_names)
                        .with_target(true);

                    tracing_subscriber::registry()
                        .with(filter)
                        .with(subscriber)
                        .init();
                }
                #[cfg(not(feature = "logging-json"))]
                {
                    // Fall back to pretty if json not enabled
                    let subscriber = fmt::layer()
                        .with_file(self.with_file)
                        .with_line_number(self.with_line_number)
                        .with_thread_ids(self.with_thread_ids)
                        .with_thread_names(self.with_thread_names)
                        .with_target(true);

                    tracing_subscriber::registry()
                        .with(filter)
                        .with(subscriber)
                        .init();
                }
            }
            LogFormat::Pretty => {
                let subscriber = fmt::layer()
                    .pretty()
                    .with_file(self.with_file)
                    .with_line_number(self.with_line_number)
                    .with_thread_ids(self.with_thread_ids)
                    .with_thread_names(self.with_thread_names)
                    .with_target(true);

                tracing_subscriber::registry()
                    .with(filter)
                    .with(subscriber)
                    .init();
            }
            LogFormat::Compact => {
                let subscriber = fmt::layer()
                    .compact()
                    .with_file(self.with_file)
                    .with_line_number(self.with_line_number)
                    .with_thread_ids(self.with_thread_ids)
                    .with_thread_names(self.with_thread_names)
                    .with_target(true);

                tracing_subscriber::registry()
                    .with(filter)
                    .with(subscriber)
                    .init();
            }
        }
    }

    /// Initialize (no-op when subscriber features not available)
    #[cfg(not(any(feature = "logging-json", feature = "logging-pretty")))]
    pub fn init(self) {
        // No-op: tracing-subscriber not enabled
        // Users should use logging-json or logging-pretty features
    }
}

/// Create a new logging builder
#[cfg(feature = "logging")]
pub fn builder() -> LoggingBuilder {
    LoggingBuilder::new()
}

/// Initialize logging with default settings
///
/// Uses JSON format if `logging-json` feature is enabled,
/// otherwise uses pretty format if `logging-pretty` is enabled.
#[cfg(any(feature = "logging-json", feature = "logging-pretty"))]
pub fn init() {
    #[cfg(feature = "logging-json")]
    {
        init_json();
    }
    #[cfg(all(feature = "logging-pretty", not(feature = "logging-json")))]
    {
        init_pretty();
    }
}

/// Initialize logging (no-op when subscriber features not available)
#[cfg(not(any(feature = "logging-json", feature = "logging-pretty")))]
pub fn init() {
    // No-op: requires logging-json or logging-pretty feature
}

/// Initialize JSON structured logging
///
/// Outputs logs in JSON format, ideal for production environments
/// where logs are aggregated and parsed by tools like ELK or Datadog.
///
/// # Example output
/// ```json
/// {"timestamp":"2024-01-01T00:00:00.000Z","level":"DEBUG","target":"dependency_injector","message":"Creating new DI container"}
/// ```
#[cfg(any(feature = "logging-json", feature = "logging-pretty"))]
pub fn init_json() {
    builder().json().debug().init();
}

/// Initialize JSON logging (no-op when not available)
#[cfg(not(any(feature = "logging-json", feature = "logging-pretty")))]
pub fn init_json() {
    // No-op: requires logging-json or logging-pretty feature
}

/// Initialize pretty colorful logging
///
/// Outputs logs in a human-readable format with colors,
/// ideal for development and debugging.
///
/// # Example output
/// ```text
///   2024-01-01T00:00:00.000Z DEBUG dependency_injector: Creating new DI container
/// ```
#[cfg(any(feature = "logging-json", feature = "logging-pretty"))]
pub fn init_pretty() {
    builder().pretty().debug().init();
}

/// Initialize pretty logging (no-op when not available)
#[cfg(not(any(feature = "logging-json", feature = "logging-pretty")))]
pub fn init_pretty() {
    // No-op: requires logging-json or logging-pretty feature
}

/// Initialize logging for dependency-injector only (filters other crates)
#[cfg(any(feature = "logging-json", feature = "logging-pretty"))]
pub fn init_di_only() {
    builder().di_only().debug().init();
}

/// Initialize DI-only logging (no-op when not available)
#[cfg(not(any(feature = "logging-json", feature = "logging-pretty")))]
pub fn init_di_only() {
    // No-op: requires logging-json or logging-pretty feature
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder = LoggingBuilder::default();
        assert_eq!(builder.level, Level::DEBUG);
        assert_eq!(builder.format, LogFormat::Json);
        assert!(builder.target.is_none());
    }

    #[test]
    fn test_builder_chain() {
        let builder = LoggingBuilder::new()
            .trace()
            .pretty()
            .with_file()
            .with_line_number()
            .di_only();

        assert_eq!(builder.level, Level::TRACE);
        assert_eq!(builder.format, LogFormat::Pretty);
        assert!(builder.with_file);
        assert!(builder.with_line_number);
        assert_eq!(builder.target, Some("dependency_injector"));
    }
}
