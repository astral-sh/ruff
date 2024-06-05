use lsp_types::{notification::Notification, TraceValue};
use serde::Deserialize;
use std::sync::{Mutex, OnceLock};
use tracing::{level_filters::LevelFilter, Event};
use tracing_subscriber::{layer::SubscriberExt, Layer};

use crate::server::ClientSender;

static LOGGING_SENDER: OnceLock<ClientSender> = OnceLock::new();

static TRACE_VALUE: Mutex<lsp_types::TraceValue> = Mutex::new(lsp_types::TraceValue::Off);

pub(crate) fn stderr_subscriber() -> impl tracing::Subscriber {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(|| Box::new(std::io::stderr())))
}

pub(crate) fn set_trace_value(trace_value: TraceValue) {
    let mut global_trace_value = TRACE_VALUE
        .lock()
        .expect("trace value mutex should be available");
    *global_trace_value = trace_value;
}

pub(crate) fn init_tracing(sender: ClientSender) {
    LOGGING_SENDER
        .set(sender)
        .expect("logging sender should only be initialized once");

    // TODO(jane): Provide a way to set the log level
    let subscriber =
        tracing_subscriber::Registry::default().with(LogLayer.with_filter(LogFilter {
            filter: LogLevel::Info,
        }));

    tracing::subscriber::set_global_default(subscriber)
        .expect("should be able to set global default subscriber");
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

struct LogLayer;

struct LogFilter {
    filter: LogLevel,
}

impl<S> tracing_subscriber::layer::Filter<S> for LogFilter {
    fn enabled(
        &self,
        meta: &tracing::Metadata<'_>,
        _: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        let filter = if meta.target().starts_with("ruff") {
            self.filter.trace_level()
        } else {
            tracing::Level::INFO
        };

        meta.level() <= &filter
    }

    fn max_level_hint(&self) -> Option<tracing::level_filters::LevelFilter> {
        Some(LevelFilter::from_level(self.filter.trace_level()))
    }
}

impl tracing_subscriber::Layer<tracing_subscriber::Registry> for LogLayer {
    fn register_callsite(
        &self,
        meta: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        if meta.is_event() && meta.fields().iter().any(|field| field.name() == "message") {
            tracing::subscriber::Interest::always()
        } else {
            tracing::subscriber::Interest::never()
        }
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
