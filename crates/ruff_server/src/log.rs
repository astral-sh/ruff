use lsp_types::notification::Notification;
use serde::Deserialize;
use std::sync::OnceLock;

use crate::server::ClientSender;

static LOGGING_SENDER: OnceLock<ClientSender> = OnceLock::new();
static LOG_FILTER: OnceLock<LogLevel> = OnceLock::new();

pub(crate) fn init_logger(sender: &ClientSender, filter: LogLevel) {
    LOGGING_SENDER
        .set(sender.clone())
        .expect("logging sender should only be initialized once");
    LOG_FILTER
        .set(filter)
        .expect("log filter should only be initialized once");
}

#[derive(Clone, Copy, Debug, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LogLevel {
    #[default]
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    fn message_type(self) -> lsp_types::MessageType {
        match self {
            Self::Error => lsp_types::MessageType::ERROR,
            Self::Warn => lsp_types::MessageType::WARNING,
            Self::Info => lsp_types::MessageType::INFO,
            // TODO(jane): LSP `0.3.18` will introduce `MessageType::DEBUG`,
            // and we should consider using that.
            Self::Debug => lsp_types::MessageType::LOG,
        }
    }
}

#[inline]
pub(crate) fn log(message: String, level: LogLevel) {
    if &level
        > LOG_FILTER
            .get()
            .expect("log filter should be be initialized")
    {
        return;
    }

    let _ = LOGGING_SENDER
        .get()
        .expect("logging channel should be initialized")
        .send(lsp_server::Message::Notification(
            lsp_server::Notification {
                method: lsp_types::notification::LogMessage::METHOD.into(),
                params: serde_json::to_value(lsp_types::LogMessageParams {
                    typ: level.message_type(),
                    message,
                })
                .expect("log notification should serialize"),
            },
        ));
}

macro_rules! log_error {
    ($msg:expr$(, $($arg:tt)*)?) => {
        crate::log::log(::core::format_args!($msg, $($($arg)*)?).to_string(), crate::log::LogLevel::Error)
    };
}

macro_rules! log_warn {
    ($msg:expr$(, $($arg:tt)*)?) => {
        crate::log::log(::core::format_args!($msg, $($($arg)*)?).to_string(), crate::log::LogLevel::Warn)
    };
}

macro_rules! log_info {
    ($msg:expr$(, $($arg:tt)*)?) => {
        crate::log::log(::core::format_args!($msg, $($($arg)*)?).to_string(), crate::log::LogLevel::Info)
    };
}

macro_rules! log_debug {
    ($msg:expr$(, $($arg:tt)*)?) => {
        crate::log::log(::core::format_args!($msg, $($($arg)*)?).to_string(), crate::log::LogLevel::Debug)
    };
}
