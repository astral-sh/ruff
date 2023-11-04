use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::CallPath;
use ruff_python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for calls to trio functions that are not immediately awaited
///
/// ## Why is this bad?
/// Not awaiting an asynchronous trio function can lead to `RuntimeWarning`s and all sorts of
/// unexpected behaviour as a result. To prevent this, calls to async trio functions should be
/// immediately awaited
///
/// ## Example
/// ```python
/// async def double_sleep(x):
///     trio.sleep(2 * x)
/// ```
///
/// Use instead:
/// ```python
/// async def double_sleep(x):
///     await trio.sleep(2 * x)
/// ```
#[violation]
pub struct SyncTrioCall {
    method_name: MethodName,
}

impl AlwaysFixableViolation for SyncTrioCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { method_name } = self;
        format!("A call to `trio` method `{method_name}` is not immediately awaited")
    }

    fn fix_title(&self) -> String {
        let Self { method_name } = self;
        format!("Await the call to `{method_name}`")
    }
}

/// TRIO105
pub(crate) fn sync_trio_call(checker: &mut Checker, call: &ExprCall) {
    let Some(method_name) = ({
        let Some(call_path) = checker.semantic().resolve_call_path(call.func.as_ref()) else {
            return;
        };
        MethodName::try_from(&call_path)
    }) else {
        return;
    };

    if checker
        .semantic()
        .current_expression_parent()
        .is_some_and(Expr::is_await_expr)
    {
        return;
    };

    let mut diagnostic = Diagnostic::new(SyncTrioCall { method_name }, call.range);
    diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
        "await ".to_string(),
        call.func.start(),
    )));
    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MethodName {
    AcloseForcefully,
    OpenFile,
    OpenSslOverTcpListeners,
    OpenSslOverTcpStream,
    OpenTcpListeners,
    OpenTcpStream,
    OpenUnixSocket,
    RunProcess,
    ServeListeners,
    ServeSslOverTcp,
    ServeTcp,
    Sleep,
    SleepForever,
    CancelShieldedCheckpoint,
    Checkpoint,
    CheckpointIfCancelled,
    OpenProcess,
    PermanentlyDetachCoroutineObject,
    ReattachDetachedCoroutineObject,
    TemporarilyDetachCoroutineObject,
    WaitReadable,
    WaitTaskRescheduled,
    WaitWritable,
}

impl MethodName {
    fn try_from(call_path: &CallPath<'_>) -> Option<Self> {
        match call_path.as_slice() {
            ["trio", "aclose_forcefully"] => Some(Self::AcloseForcefully),
            ["trio", "open_file"] => Some(Self::OpenFile),
            ["trio", "open_ssl_over_tcp_listeners"] => Some(Self::OpenSslOverTcpListeners),
            ["trio", "open_ssl_over_tcp_stream"] => Some(Self::OpenSslOverTcpStream),
            ["trio", "open_tcp_listeners"] => Some(Self::OpenTcpListeners),
            ["trio", "open_tcp_stream"] => Some(Self::OpenTcpStream),
            ["trio", "open_unix_socket"] => Some(Self::OpenUnixSocket),
            ["trio", "run_process"] => Some(Self::RunProcess),
            ["trio", "serve_listeners"] => Some(Self::ServeListeners),
            ["trio", "serve_ssl_over_tcp"] => Some(Self::ServeSslOverTcp),
            ["trio", "serve_tcp"] => Some(Self::ServeTcp),
            ["trio", "sleep"] => Some(Self::Sleep),
            ["trio", "sleep_forever"] => Some(Self::SleepForever),
            ["trio", "lowlevel", "cancel_shielded_checkpoint"] => {
                Some(Self::CancelShieldedCheckpoint)
            }
            ["trio", "lowlevel", "checkpoint"] => Some(Self::Checkpoint),
            ["trio", "lowlevel", "checkpoint_if_cancelled"] => Some(Self::CheckpointIfCancelled),
            ["trio", "lowlevel", "open_process"] => Some(Self::OpenProcess),
            ["trio", "lowlevel", "permanently_detach_coroutine_object"] => {
                Some(Self::PermanentlyDetachCoroutineObject)
            }
            ["trio", "lowlevel", "reattach_detached_coroutine_object"] => {
                Some(Self::ReattachDetachedCoroutineObject)
            }
            ["trio", "lowlevel", "temporarily_detach_coroutine_object"] => {
                Some(Self::TemporarilyDetachCoroutineObject)
            }
            ["trio", "lowlevel", "wait_readable"] => Some(Self::WaitReadable),
            ["trio", "lowlevel", "wait_task_rescheduled"] => Some(Self::WaitTaskRescheduled),
            ["trio", "lowlevel", "wait_writable"] => Some(Self::WaitWritable),
            _ => None,
        }
    }
}

impl std::fmt::Display for MethodName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MethodName::AcloseForcefully => write!(f, "trio.aclose_forcefully"),
            MethodName::OpenFile => write!(f, "trio.open_file"),
            MethodName::OpenSslOverTcpListeners => write!(f, "trio.open_ssl_over_tcp_listeners"),
            MethodName::OpenSslOverTcpStream => write!(f, "trio.open_ssl_over_tcp_stream"),
            MethodName::OpenTcpListeners => write!(f, "trio.open_tcp_listeners"),
            MethodName::OpenTcpStream => write!(f, "trio.open_tcp_stream"),
            MethodName::OpenUnixSocket => write!(f, "trio.open_unix_socket"),
            MethodName::RunProcess => write!(f, "trio.run_process"),
            MethodName::ServeListeners => write!(f, "trio.serve_listeners"),
            MethodName::ServeSslOverTcp => write!(f, "trio.serve_ssl_over_tcp"),
            MethodName::ServeTcp => write!(f, "trio.serve_tcp"),
            MethodName::Sleep => write!(f, "trio.sleep"),
            MethodName::SleepForever => write!(f, "trio.sleep_forever"),
            MethodName::CancelShieldedCheckpoint => {
                write!(f, "trio.lowlevel.cancel_shielded_checkpoint")
            }
            MethodName::Checkpoint => write!(f, "trio.lowlevel.checkpoint"),
            MethodName::CheckpointIfCancelled => write!(f, "trio.lowlevel.checkpoint_if_cancelled"),
            MethodName::OpenProcess => write!(f, "trio.lowlevel.open_process"),
            MethodName::PermanentlyDetachCoroutineObject => {
                write!(f, "trio.lowlevel.permanently_detach_coroutine_object")
            }
            MethodName::ReattachDetachedCoroutineObject => {
                write!(f, "trio.lowlevel.reattach_detached_coroutine_object")
            }
            MethodName::TemporarilyDetachCoroutineObject => {
                write!(f, "trio.lowlevel.temporarily_detach_coroutine_object")
            }
            MethodName::WaitReadable => write!(f, "trio.lowlevel.wait_readable"),
            MethodName::WaitTaskRescheduled => write!(f, "trio.lowlevel.wait_task_rescheduled"),
            MethodName::WaitWritable => write!(f, "trio.lowlevel.wait_writable"),
        }
    }
}
