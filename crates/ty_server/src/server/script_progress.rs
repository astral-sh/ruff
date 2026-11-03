use std::sync::{Arc, Mutex, Weak};

use lsp_types::WorkDoneProgressBegin;
use ty_project::UvSyncProgress;

use crate::capabilities::ResolvedClientCapabilities;
use crate::session::client::Client;

use super::LazyWorkDoneProgress;

/// Shows completed/total script synchronizations and the last started script in one indicator.
///
/// The indicator starts when the first request is scheduled, including time spent queued.
/// Only request guards own the shared state. The session keeps a weak reference so the indicator
/// ends when the last request is applied or dropped, including requests that have not started yet.
#[derive(Clone, Default)]
pub(crate) struct ScriptProgress {
    current: Arc<Mutex<Weak<SharedProgress>>>,
}

impl ScriptProgress {
    pub(crate) fn for_script(
        &self,
        client: &Client,
        capabilities: ResolvedClientCapabilities,
        display_path: String,
    ) -> Option<Box<dyn UvSyncProgress>> {
        if !capabilities.supports_work_done_progress() {
            return None;
        }

        let mut current = self.current.lock().ok()?;
        let shared = current.upgrade().unwrap_or_else(|| {
            let shared = Arc::new(SharedProgress {
                work_done: LazyWorkDoneProgress::new_on_main_loop(
                    client,
                    WorkDoneProgressBegin {
                        title: "Synchronizing scripts".to_string(),
                        cancellable: Some(false),
                        message: Some("0/1".to_string()),
                        percentage: None,
                    },
                    capabilities,
                ),
                state: Mutex::default(),
            });
            *current = Arc::downgrade(&shared);
            shared
        });
        {
            let mut state = shared.state.lock().ok()?;
            state.total += 1;
            state.report_progress(&shared.work_done);
        }

        Some(Box::new(ScriptProgressGuard {
            shared,
            name: display_path,
        }))
    }
}

struct ScriptProgressGuard {
    shared: Arc<SharedProgress>,
    name: String,
}

impl UvSyncProgress for ScriptProgressGuard {
    fn started(&mut self) {
        let Ok(mut state) = self.shared.state.lock() else {
            return;
        };
        state.last_started.clone_from(&self.name);
        state.report_progress(&self.shared.work_done);
    }

    fn completed(self: Box<Self>) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.completed += 1;
            state.report_progress(&self.shared.work_done);
        }
    }
}

struct SharedProgress {
    work_done: LazyWorkDoneProgress,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    completed: usize,
    total: usize,
    last_started: String,
}

impl State {
    fn report_progress(&self, progress: &LazyWorkDoneProgress) {
        let mut message = format!("{}/{}", self.completed, self.total);
        if !self.last_started.is_empty() {
            message.push_str(": ");
            message.push_str(&self.last_started);
        }
        progress.report_progress(message, None);
        if self.completed == self.total {
            progress.set_finish_message("Finished synchronizing scripts".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Context, Result, bail};
    use crossbeam::channel::unbounded;
    use lsp_server::Message;
    use lsp_types::{ProgressParams, ProgressToken};

    use crate::capabilities::ResolvedClientCapabilities;
    use crate::server::LazyWorkDoneProgress;
    use crate::session::client::Client;

    use super::State;

    #[test]
    fn script_progress_reports_completion_counts_and_last_started() -> Result<()> {
        let (main_loop, actions) = unbounded();
        let (sender, messages) = unbounded();
        let client = Client::new(main_loop, sender);
        let work_done = LazyWorkDoneProgress::new(
            &client,
            Some(ProgressToken::String("scripts".into())),
            "Synchronizing scripts",
            ResolvedClientCapabilities::WORK_DONE_PROGRESS,
        );
        let mut state = State {
            total: 2,
            ..State::default()
        };
        state.report_progress(&work_done);
        state.last_started = "first.py".into();
        state.report_progress(&work_done);
        state.last_started = "second.py".into();
        state.report_progress(&work_done);
        state.completed = 1;
        state.report_progress(&work_done);
        state.completed = 2;
        state.report_progress(&work_done);
        drop(work_done);

        assert_eq!(
            messages
                .try_iter()
                .map(progress_notification)
                .collect::<Result<Vec<_>>>()?,
            [
                "begin: ",
                "report: 0/2",
                "report: 0/2: first.py",
                "report: 0/2: second.py",
                "report: 1/2: second.py",
                "report: 2/2: second.py",
                "end: Finished synchronizing scripts",
            ]
        );
        assert!(actions.is_empty());
        Ok(())
    }

    fn progress_notification(message: Message) -> Result<String> {
        let Message::Notification(notification) = message else {
            bail!("expected progress notification");
        };
        let params: ProgressParams = serde_json::from_value(notification.params)?;
        let kind = params.value["kind"]
            .as_str()
            .context("missing progress kind")?;
        let message = params.value["message"].as_str().unwrap_or_default();
        Ok(format!("{kind}: {message}"))
    }
}
