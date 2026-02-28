//! Extension mechanism for `ty_server`.
//!
//! This module provides traits that allow external code to:
//! - Handle custom JSON-RPC methods that `ty_server` doesn't natively support
//! - React to document change notifications
//! - Send custom notifications to the client
//!
//! This is primarily used by `tsp_ty_server` to add TSP protocol support.

use lsp_server::{Request, Response};
use ty_project::ProjectDatabase;

/// A trait for sending notifications to the LSP client.
///
/// This is passed to extension callbacks so they can send custom notifications
/// (e.g., snapshotChanged) in response to document events.
pub trait Notifier: Send + Sync {
    /// Send a notification with the given method and params.
    fn send_notification(&self, method: &str, params: serde_json::Value);
}

/// A trait for handling extension requests and reacting to notifications.
///
/// Implementors of this trait can:
/// - Handle custom JSON-RPC methods that `ty_server` doesn't natively support
///   (the extension gets **first look** at incoming requests).
/// - React to notifications **after** `ty_server` has processed them
///   (e.g., to update internal state or send custom notifications).
///
/// # Thread Safety
///
/// Extensions must be `Send + Sync` because they may be called from
/// background threads.
///
/// # Request Scheduling
///
/// Extension requests run on a **background thread** (not the main loop),
/// so they won't block other LSP operations like hover, completions, etc.
/// The extension receives read-only snapshots of the project databases.
pub trait RequestExtension: Send + Sync {
    /// Check if this extension handles the given request method.
    ///
    /// Returns `true` if the extension wants to handle this method,
    /// `false` to let `ty_server` return an "unknown method" error.
    fn handles_method(&self, method: &str) -> bool;

    /// Handle a request on a background thread.
    ///
    /// This is called when `handles_method` returns `true`. The extension
    /// should process the request and return a response.
    ///
    /// # Arguments
    ///
    /// * `request` - The incoming JSON-RPC request
    /// * `databases` - Read-only snapshots of all project databases
    ///
    /// # Returns
    ///
    /// A response to send back to the client, or `None` if the extension
    /// cannot handle this particular request.
    fn handle_request(&self, request: &Request, databases: &[ProjectDatabase]) -> Option<Response>;

    /// Called after `ty_server` has processed a notification.
    ///
    /// Extensions can use this to react to any notification (e.g.,
    /// `textDocument/didOpen`, `textDocument/didChange`, `textDocument/didClose`)
    /// by updating internal state or sending custom notifications to the client.
    ///
    /// The extension decides which notification methods it cares about;
    /// uninteresting methods can simply be ignored.
    ///
    /// # Arguments
    ///
    /// * `method` - The notification method name (e.g., `"textDocument/didOpen"`)
    /// * `notifier` - A handle for sending custom notifications to the client
    fn on_notification(
        &self,
        _method: &str,
        _notifier: &dyn Notifier,
    ) {
    }
}

/// A no-op extension that doesn't handle any methods.
///
/// This is the default extension used when no custom extension is provided.
#[derive(Debug, Default)]
pub struct NoOpExtension;

impl RequestExtension for NoOpExtension {
    fn handles_method(&self, _method: &str) -> bool {
        false
    }

    fn handle_request(&self, _request: &Request, _databases: &[ProjectDatabase]) -> Option<Response> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use lsp_server::{Request, RequestId, Response};
    use serde_json::json;

    use super::{NoOpExtension, Notifier, RequestExtension};

    // -- Test helpers -------------------------------------------------------

    /// A mock notifier that records all notifications sent through it.
    struct MockNotifier {
        sent: Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl MockNotifier {
        fn new() -> Self {
            Self {
                sent: Mutex::new(Vec::new()),
            }
        }

        fn sent_notifications(&self) -> Vec<(String, serde_json::Value)> {
            self.sent.lock().unwrap().clone()
        }
    }

    impl Notifier for MockNotifier {
        fn send_notification(&self, method: &str, params: serde_json::Value) {
            self.sent
                .lock()
                .unwrap()
                .push((method.to_string(), params));
        }
    }

    /// A test extension that handles a fixed set of methods and records
    /// notification callbacks.
    struct TestExtension {
        /// Methods this extension claims to handle.
        methods: Vec<String>,
        /// Notification methods received via on_notification.
        notifications: Mutex<Vec<String>>,
    }

    impl TestExtension {
        fn new(methods: Vec<&str>) -> Self {
            Self {
                methods: methods.into_iter().map(String::from).collect(),
                notifications: Mutex::new(Vec::new()),
            }
        }

        fn received_notifications(&self) -> Vec<String> {
            self.notifications.lock().unwrap().clone()
        }
    }

    impl RequestExtension for TestExtension {
        fn handles_method(&self, method: &str) -> bool {
            self.methods.iter().any(|m| m == method)
        }

        fn handle_request(
            &self,
            request: &Request,
            _databases: &[ty_project::ProjectDatabase],
        ) -> Option<Response> {
            // Return a success response echoing the method name.
            Some(Response::new_ok(
                request.id.clone(),
                json!({ "handled_method": request.method }),
            ))
        }

        fn on_notification(
            &self,
            method: &str,
            _notifier: &dyn Notifier,
        ) {
            self.notifications
                .lock()
                .unwrap()
                .push(method.to_string());
        }
    }

    #[allow(dead_code)]
    fn make_request(id: i32, method: &str) -> Request {
        Request {
            id: RequestId::from(id),
            method: method.to_string(),
            params: json!({}),
        }
    }

    // -- NoOpExtension tests ------------------------------------------------

    #[test]
    fn noop_extension_does_not_handle_any_method() {
        let ext = NoOpExtension;
        assert!(!ext.handles_method("custom/myMethod"));
        assert!(!ext.handles_method("textDocument/hover"));
        assert!(!ext.handles_method(""));
    }

    #[test]
    fn noop_extension_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NoOpExtension>();
    }

    // -- Custom extension: handles_method -----------------------------------

    #[test]
    fn custom_extension_handles_registered_methods() {
        let ext = TestExtension::new(vec!["custom/foo", "custom/bar"]);
        assert!(ext.handles_method("custom/foo"));
        assert!(ext.handles_method("custom/bar"));
        assert!(!ext.handles_method("custom/baz"));
        assert!(!ext.handles_method("textDocument/hover"));
    }

    #[test]
    fn custom_extension_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TestExtension>();
    }

    // -- Extension behind Arc (as used in server dispatch) ------------------

    #[test]
    fn extension_works_behind_arc() {
        let ext: Arc<dyn RequestExtension> =
            Arc::new(TestExtension::new(vec!["custom/test"]));
        assert!(ext.handles_method("custom/test"));
        assert!(!ext.handles_method("other"));
    }

    // -- on_notification callback -------------------------------------------

    #[test]
    fn extension_receives_notification_callback() {
        let ext = TestExtension::new(vec![]);
        let notifier = MockNotifier::new();

        ext.on_notification("textDocument/didOpen", &notifier);

        let received = ext.received_notifications();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0], "textDocument/didOpen");
    }

    #[test]
    fn extension_receives_multiple_notification_types() {
        let ext = TestExtension::new(vec![]);
        let notifier = MockNotifier::new();

        ext.on_notification("textDocument/didOpen", &notifier);
        ext.on_notification("textDocument/didChange", &notifier);
        ext.on_notification("textDocument/didClose", &notifier);

        let received = ext.received_notifications();
        assert_eq!(received.len(), 3);
        assert_eq!(received[0], "textDocument/didOpen");
        assert_eq!(received[1], "textDocument/didChange");
        assert_eq!(received[2], "textDocument/didClose");
    }

    #[test]
    fn noop_extension_on_notification_is_silent() {
        let ext = NoOpExtension;
        let notifier = MockNotifier::new();

        // These should not panic — they're no-ops by default.
        ext.on_notification("textDocument/didOpen", &notifier);
        ext.on_notification("textDocument/didChange", &notifier);
        ext.on_notification("textDocument/didClose", &notifier);

        // No notifications should have been sent.
        assert!(notifier.sent_notifications().is_empty());
    }

    // -- Notifier trait tests -----------------------------------------------

    #[test]
    fn mock_notifier_records_notifications() {
        let notifier = MockNotifier::new();

        notifier.send_notification("custom/event", json!({ "key": "value" }));
        notifier.send_notification("custom/other", json!(42));

        let sent = notifier.sent_notifications();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].0, "custom/event");
        assert_eq!(sent[0].1, json!({ "key": "value" }));
        assert_eq!(sent[1].0, "custom/other");
        assert_eq!(sent[1].1, json!(42));
    }

    #[test]
    fn extension_can_send_notifications_from_on_notification_callback() {
        /// An extension that sends a notification whenever it sees didOpen.
        struct NotifyingExtension;

        impl RequestExtension for NotifyingExtension {
            fn handles_method(&self, _method: &str) -> bool {
                false
            }

            fn handle_request(
                &self,
                _request: &Request,
                _databases: &[ty_project::ProjectDatabase],
            ) -> Option<Response> {
                None
            }

            fn on_notification(
                &self,
                method: &str,
                notifier: &dyn Notifier,
            ) {
                if method == "textDocument/didOpen" {
                    notifier.send_notification(
                        "custom/documentTracked",
                        json!({ "tracked": true }),
                    );
                }
            }
        }

        let ext = NotifyingExtension;
        let notifier = MockNotifier::new();

        ext.on_notification("textDocument/didOpen", &notifier);

        let sent = notifier.sent_notifications();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "custom/documentTracked");
        assert_eq!(sent[0].1, json!({ "tracked": true }));
    }

    // -- Multiple lifecycle events ------------------------------------------

    #[test]
    fn extension_tracks_full_document_lifecycle() {
        let ext = TestExtension::new(vec![]);
        let notifier = MockNotifier::new();

        ext.on_notification("textDocument/didOpen", &notifier);
        ext.on_notification("textDocument/didChange", &notifier);
        ext.on_notification("textDocument/didChange", &notifier);
        ext.on_notification("textDocument/didClose", &notifier);

        let received = ext.received_notifications();
        assert_eq!(received.len(), 4);
    }
}
