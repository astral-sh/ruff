use std::sync::OnceLock;

use lsp_types::notification::Notification;

use crate::server::ClientSender;

static MESSENGER: OnceLock<ClientSender> = OnceLock::new();

pub(crate) fn init_messenger(client_sender: ClientSender) {
    MESSENGER
        .set(client_sender)
        .expect("messenger should only be initialized once");

    // unregister any previously registered panic hook
    let _ = std::panic::take_hook();

    // When we panic, try to notify the client.
    std::panic::set_hook(Box::new(move |panic_info| {
        if let Some(messenger) = MESSENGER.get() {
            let _ = messenger.send(lsp_server::Message::Notification(
                lsp_server::Notification {
                    method: lsp_types::notification::ShowMessage::METHOD.into(),
                    params: serde_json::to_value(lsp_types::ShowMessageParams {
                        typ: lsp_types::MessageType::ERROR,
                        message: String::from(
                            "The Ruff language server exited with a panic. See the logs for more details."
                        ),
                    })
                    .unwrap_or_default(),
                },
            ));
        }

        let backtrace = std::backtrace::Backtrace::force_capture();
        tracing::error!("{panic_info}\n{backtrace}");
        #[allow(clippy::print_stderr)]
        {
            // we also need to print to stderr directly in case tracing hasn't
            // been initialized.
            eprintln!("{panic_info}\n{backtrace}");
        }
    }));
}

pub(crate) fn show_message(message: String, message_type: lsp_types::MessageType) {
    MESSENGER
        .get()
        .expect("messenger should be initialized")
        .send(lsp_server::Message::Notification(
            lsp_server::Notification {
                method: lsp_types::notification::ShowMessage::METHOD.into(),
                params: serde_json::to_value(lsp_types::ShowMessageParams {
                    typ: message_type,
                    message,
                })
                .unwrap(),
            },
        ))
        .expect("message should send");
}

/// Sends an error to the client with a formatted message. The error is sent in a
/// `window/showMessage` notification.
macro_rules! show_err_msg {
    ($msg:expr$(, $($arg:tt),*)?) => {
        crate::message::show_message(::core::format_args!($msg, $($($arg),*)?).to_string(), lsp_types::MessageType::ERROR)
    };
}
