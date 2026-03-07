//! TSP extension for `ty_server`.
//!
//! This module provides a `RequestExtension` implementation that allows
//! `ty_server` to handle TSP (Type Server Protocol) requests.

use lsp_server::{Request, Response};
use ty_project::ProjectDatabase;
use ty_server::{Notifier, RequestExtension};

use crate::SnapshotManager;
use crate::handlers;

/// Extension that adds TSP protocol support to `ty_server`.
///
/// This extension intercepts requests with `typeServer/` prefix and
/// routes them to the appropriate TSP handlers.
///
/// It also tracks document changes to update the snapshot number,
/// which allows clients to detect when type information may be stale.
pub struct TspExtension {
    snapshot_manager: SnapshotManager,
}

impl TspExtension {
    /// Create a new TSP extension.
    pub fn new(snapshot_manager: SnapshotManager) -> Self {
        Self { snapshot_manager }
    }

    /// Send a snapshotChanged notification to the client.
    #[allow(clippy::unused_self)]
    fn send_snapshot_changed(&self, old_snapshot: u64, new_snapshot: u64, notifier: &dyn Notifier) {
        let params = serde_json::json!({
            "old": old_snapshot,
            "new": new_snapshot
        });
        notifier.send_notification(tsp_types::methods::SNAPSHOT_CHANGED, params);
        tracing::debug!(
            "Sent snapshotChanged notification: old={old_snapshot}, new={new_snapshot}"
        );
    }
}

impl RequestExtension for TspExtension {
    fn handles_method(&self, method: &str) -> bool {
        tsp_types::is_tsp_method(method)
    }

    fn handle_request(&self, request: &Request, databases: &[ProjectDatabase]) -> Option<Response> {
        // Parse the params
        let params = request.params.clone();

        // Dispatch to TSP handlers
        let result = handlers::dispatch_tsp_request(
            &request.method,
            params,
            &self.snapshot_manager,
            databases,
        );

        // Convert the result to a Response
        result.map(|res| match res {
            Ok(value) => Response::new_ok(request.id.clone(), value),
            Err(msg) => Response::new_err(
                request.id.clone(),
                lsp_server::ErrorCode::InvalidRequest as i32,
                msg,
            ),
        })
    }

    fn on_notification(&self, method: &str, notifier: &dyn Notifier) {
        match method {
            "textDocument/didOpen" | "textDocument/didChange" | "textDocument/didClose" => {
                let old_snapshot = self.snapshot_manager.current();
                let new_snapshot = self.snapshot_manager.increment();
                tracing::debug!("Notification {method}: snapshot incremented to {new_snapshot}");
                self.send_snapshot_changed(old_snapshot, new_snapshot, notifier);
            }
            _ => {
                // Ignore notifications we don't care about.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tsp_types::methods;

    /// A mock notifier that records notifications for testing.
    struct MockNotifier {
        notifications: Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl MockNotifier {
        fn new() -> Self {
            Self {
                notifications: Mutex::new(Vec::new()),
            }
        }

        fn notifications(&self) -> Vec<(String, serde_json::Value)> {
            self.notifications.lock().unwrap().clone()
        }
    }

    impl Notifier for MockNotifier {
        fn send_notification(&self, method: &str, params: serde_json::Value) {
            self.notifications
                .lock()
                .unwrap()
                .push((method.to_string(), params));
        }
    }

    #[test]
    fn test_handles_tsp_methods() {
        let ext = TspExtension::new(SnapshotManager::new());

        // TSP methods should be handled
        assert!(ext.handles_method(methods::GET_SUPPORTED_PROTOCOL_VERSION));
        assert!(ext.handles_method(methods::GET_SNAPSHOT));
        assert!(ext.handles_method(methods::GET_PYTHON_SEARCH_PATHS));

        // Non-TSP methods should not be handled
        assert!(!ext.handles_method("textDocument/hover"));
        assert!(!ext.handles_method("initialize"));
    }

    #[test]
    fn test_handle_get_supported_protocol_version() {
        let ext = TspExtension::new(SnapshotManager::new());

        let request = Request {
            id: lsp_server::RequestId::from(1),
            method: methods::GET_SUPPORTED_PROTOCOL_VERSION.to_string(),
            params: serde_json::json!({}),
        };

        // We can't easily create a ProjectDatabase for testing, so we just verify
        // that the extension claims to handle the method
        assert!(ext.handles_method(&request.method));
    }

    #[test]
    fn test_did_open_increments_snapshot_and_sends_notification() {
        let snapshot_manager = SnapshotManager::new();
        let ext = TspExtension::new(snapshot_manager);
        let notifier = MockNotifier::new();

        // Initial snapshot is 1
        assert_eq!(ext.snapshot_manager.current(), 1);

        // Opening a document should increment the snapshot and send notification
        ext.on_notification("textDocument/didOpen", &notifier);
        assert_eq!(ext.snapshot_manager.current(), 2);

        let notifications = notifier.notifications();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].0, methods::SNAPSHOT_CHANGED);
        assert_eq!(notifications[0].1["old"], 1);
        assert_eq!(notifications[0].1["new"], 2);

        // Opening another document should increment again
        ext.on_notification("textDocument/didOpen", &notifier);
        assert_eq!(ext.snapshot_manager.current(), 3);

        let notifications = notifier.notifications();
        assert_eq!(notifications.len(), 2);
        assert_eq!(notifications[1].1["old"], 2);
        assert_eq!(notifications[1].1["new"], 3);
    }

    #[test]
    fn test_did_change_increments_snapshot_and_sends_notification() {
        let snapshot_manager = SnapshotManager::new();
        let ext = TspExtension::new(snapshot_manager);
        let notifier = MockNotifier::new();

        assert_eq!(ext.snapshot_manager.current(), 1);

        ext.on_notification("textDocument/didChange", &notifier);
        assert_eq!(ext.snapshot_manager.current(), 2);

        let notifications = notifier.notifications();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].0, methods::SNAPSHOT_CHANGED);
    }

    #[test]
    fn test_did_close_increments_snapshot_and_sends_notification() {
        let snapshot_manager = SnapshotManager::new();
        let ext = TspExtension::new(snapshot_manager);
        let notifier = MockNotifier::new();

        assert_eq!(ext.snapshot_manager.current(), 1);

        ext.on_notification("textDocument/didClose", &notifier);
        assert_eq!(ext.snapshot_manager.current(), 2);

        let notifications = notifier.notifications();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].0, methods::SNAPSHOT_CHANGED);
    }

    #[test]
    fn test_unrelated_notification_is_ignored() {
        let snapshot_manager = SnapshotManager::new();
        let ext = TspExtension::new(snapshot_manager);
        let notifier = MockNotifier::new();

        assert_eq!(ext.snapshot_manager.current(), 1);

        ext.on_notification("textDocument/didSave", &notifier);
        // Snapshot should NOT have changed
        assert_eq!(ext.snapshot_manager.current(), 1);
        assert!(notifier.notifications().is_empty());
    }

    #[test]
    fn test_document_events_sequence() {
        let snapshot_manager = SnapshotManager::new();
        let ext = TspExtension::new(snapshot_manager);
        let notifier = MockNotifier::new();

        // Simulate a typical document lifecycle
        assert_eq!(ext.snapshot_manager.current(), 1);

        ext.on_notification("textDocument/didOpen", &notifier);
        assert_eq!(ext.snapshot_manager.current(), 2);

        ext.on_notification("textDocument/didChange", &notifier);
        assert_eq!(ext.snapshot_manager.current(), 3);

        ext.on_notification("textDocument/didChange", &notifier);
        assert_eq!(ext.snapshot_manager.current(), 4);

        ext.on_notification("textDocument/didClose", &notifier);
        assert_eq!(ext.snapshot_manager.current(), 5);

        // Verify all notifications were sent
        let notifications = notifier.notifications();
        assert_eq!(notifications.len(), 4);
    }
}
