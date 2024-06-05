use lsp_types::notification::Notification;
use serde::Deserialize;
use std::sync::OnceLock;
use tracing::{level_filters::LevelFilter, Event};
use tracing_subscriber::layer::SubscriberExt;

use crate::server::ClientSender;

static LOGGING_SENDER: OnceLock<ClientSender> = OnceLock::new();

pub(crate) fn init_tracing(
    sender: ClientSender,
    filter: LogLevel,
) -> tracing::subscriber::DefaultGuard {
    LOGGING_SENDER
        .set(sender)
        .expect("logging sender should only be initialized once");

    let subscriber = tracing_subscriber::Registry::default().with(LogLayer { filter });

    tracing::subscriber::set_default(subscriber)
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
    fn from_trace_level(level: tracing::Level) -> Self {
        match level {
            tracing::Level::ERROR => Self::Error,
            tracing::Level::WARN => Self::Warn,
            tracing::Level::INFO => Self::Info,
            _ => Self::Debug,
        }
    }

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

    fn trace_level(self) -> tracing::Level {
        match self {
            Self::Error => tracing::Level::ERROR,
            Self::Warn => tracing::Level::WARN,
            Self::Info => tracing::Level::INFO,
            Self::Debug => tracing::Level::DEBUG,
        }
    }
}

struct LogLayer {
    filter: LogLevel,
}

impl tracing_subscriber::Layer<tracing_subscriber::Registry> for LogLayer {
    fn register_callsite(
        &self,
        metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        if metadata.is_event()
            && metadata
                .fields()
                .iter()
                .any(|field| field.name() == "message")
        {
            tracing::subscriber::Interest::always()
        } else {
            tracing::subscriber::Interest::never()
        }
    }

    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        Some(LevelFilter::from_level(self.filter.trace_level()))
    }

    fn on_event(
        &self,
        event: &Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, tracing_subscriber::Registry>,
    ) {
        let mut message_visitor =
            LogMessageVisitor(LogLevel::from_trace_level(*event.metadata().level()));
        event.record(&mut message_visitor);
    }
}

#[derive(Default)]
struct LogMessageVisitor(LogLevel);

impl tracing::field::Visit for LogMessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            log(format!("{value:?}"), self.0);
        }
    }
}

#[inline]
fn log(message: String, level: LogLevel) {
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
