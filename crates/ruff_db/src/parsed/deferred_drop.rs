use std::sync::{Arc, OnceLock, mpsc};

use super::indexed::IndexedModule;

/// Releases large ASTs on the worker, falling back to the caller if unavailable or full.
pub(super) fn enqueue(parsed: Arc<IndexedModule>) {
    // For small modules, waking the worker costs more than dropping the AST here.
    // Token count is a cheap approximation of the amount of allocated AST data.
    if parsed.parsed.tokens().len() <= 64 {
        return;
    }
    if let Some(queue) = queue() {
        queue.enqueue(parsed);
    }
}

/// Waits for destruction of all ASTs queued before this call.
#[cfg(test)]
pub(super) fn wait() {
    if let Some(queue) = queue() {
        queue.wait();
    }
}

fn queue() -> Option<&'static DropQueue> {
    // Start only when an AST needs destruction. Platforms without threads and
    // failures to start the worker retain synchronous destruction.
    static QUEUE: OnceLock<Option<DropQueue>> = OnceLock::new();
    QUEUE.get_or_init(|| DropQueue::new().ok()).as_ref()
}

struct DropQueue {
    sender: mpsc::SyncSender<Message>,
}

impl DropQueue {
    fn new() -> std::io::Result<Self> {
        // Bound the number of retained ASTs when eviction outpaces destruction.
        // A full queue falls back to synchronous destruction on the caller.
        let (sender, receiver) = mpsc::sync_channel(1024);
        std::thread::Builder::new()
            .name("ty-ast-drop".into())
            .stack_size(crate::STACK_SIZE)
            .spawn(move || {
                for message in receiver {
                    match message {
                        Message::Drop(parsed) => drop(parsed),
                        #[cfg(test)]
                        Message::Barrier(sender) => {
                            let _ = sender.send(());
                        }
                    }
                }
            })?;
        Ok(Self { sender })
    }

    fn enqueue(&self, parsed: Arc<IndexedModule>) {
        // On a full or disconnected queue, the error owns the AST and drops it here.
        let _ = self.sender.try_send(Message::Drop(parsed));
    }

    #[cfg(test)]
    fn wait(&self) {
        let (done, receiver) = mpsc::channel();
        if self.sender.send(Message::Barrier(done)).is_ok() {
            let _ = receiver.recv();
        }
    }
}

enum Message {
    Drop(Arc<IndexedModule>),
    #[cfg(test)]
    Barrier(mpsc::Sender<()>),
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};

    use ruff_python_parser::parse_module;

    use super::{DropQueue, IndexedModule};

    #[test]
    fn worker_releases_ast() {
        let queue = DropQueue::new().expect("Start AST destruction worker");
        let parsed = parsed();
        let weak = Arc::downgrade(&parsed);
        queue.enqueue(parsed);
        queue.wait();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn full_queue_releases_ast_on_caller() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let queue = DropQueue { sender };
        queue.enqueue(parsed());

        let parsed = parsed();
        let weak = Arc::downgrade(&parsed);
        queue.enqueue(parsed);
        assert!(weak.upgrade().is_none());
        drop(receiver);
    }

    #[test]
    fn disconnected_queue_releases_ast_on_caller() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let queue = DropQueue { sender };
        drop(receiver);

        let parsed = parsed();
        let weak = Arc::downgrade(&parsed);
        queue.enqueue(parsed);
        assert!(weak.upgrade().is_none());
    }

    fn parsed() -> Arc<IndexedModule> {
        IndexedModule::new(parse_module("value = [1, 2, 3]").expect("Parse module"))
    }
}
