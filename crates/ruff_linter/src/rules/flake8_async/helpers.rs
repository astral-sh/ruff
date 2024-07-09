use ruff_python_ast::name::QualifiedName;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum AsyncModule {
    /// `anyio`
    AnyIo,
    /// `asyncio`
    AsyncIo,
    /// `trio`
    Trio,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum MethodName {
    AcloseForcefully,
    CancelScope,
    CancelShieldedCheckpoint,
    Checkpoint,
    CheckpointIfCancelled,
    FailAfter,
    FailAt,
    MoveOnAfter,
    MoveOnAt,
    OpenFile,
    OpenProcess,
    OpenSslOverTcpListeners,
    OpenSslOverTcpStream,
    OpenTcpListeners,
    OpenTcpStream,
    OpenUnixSocket,
    PermanentlyDetachCoroutineObject,
    ReattachDetachedCoroutineObject,
    RunProcess,
    ServeListeners,
    ServeSslOverTcp,
    ServeTcp,
    Sleep,
    SleepForever,
    TemporarilyDetachCoroutineObject,
    WaitReadable,
    WaitTaskRescheduled,
    WaitWritable,
}

impl MethodName {
    /// Returns `true` if the method is async, `false` if it is sync.
    pub(super) fn is_async(self) -> bool {
        match self {
            MethodName::AcloseForcefully
            | MethodName::CancelShieldedCheckpoint
            | MethodName::Checkpoint
            | MethodName::CheckpointIfCancelled
            | MethodName::OpenFile
            | MethodName::OpenProcess
            | MethodName::OpenSslOverTcpListeners
            | MethodName::OpenSslOverTcpStream
            | MethodName::OpenTcpListeners
            | MethodName::OpenTcpStream
            | MethodName::OpenUnixSocket
            | MethodName::PermanentlyDetachCoroutineObject
            | MethodName::ReattachDetachedCoroutineObject
            | MethodName::RunProcess
            | MethodName::ServeListeners
            | MethodName::ServeSslOverTcp
            | MethodName::ServeTcp
            | MethodName::Sleep
            | MethodName::SleepForever
            | MethodName::TemporarilyDetachCoroutineObject
            | MethodName::WaitReadable
            | MethodName::WaitTaskRescheduled
            | MethodName::WaitWritable => true,

            MethodName::MoveOnAfter
            | MethodName::MoveOnAt
            | MethodName::FailAfter
            | MethodName::FailAt
            | MethodName::CancelScope => false,
        }
    }
}

impl MethodName {
    pub(super) fn try_from(qualified_name: &QualifiedName<'_>) -> Option<Self> {
        match qualified_name.segments() {
            ["trio", "CancelScope"] => Some(Self::CancelScope),
            ["trio", "aclose_forcefully"] => Some(Self::AcloseForcefully),
            ["trio", "fail_after"] => Some(Self::FailAfter),
            ["trio", "fail_at"] => Some(Self::FailAt),
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
            ["trio", "move_on_after"] => Some(Self::MoveOnAfter),
            ["trio", "move_on_at"] => Some(Self::MoveOnAt),
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
            _ => None,
        }
    }
}

impl std::fmt::Display for MethodName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MethodName::AcloseForcefully => write!(f, "trio.aclose_forcefully"),
            MethodName::CancelScope => write!(f, "trio.CancelScope"),
            MethodName::CancelShieldedCheckpoint => {
                write!(f, "trio.lowlevel.cancel_shielded_checkpoint")
            }
            MethodName::Checkpoint => write!(f, "trio.lowlevel.checkpoint"),
            MethodName::CheckpointIfCancelled => write!(f, "trio.lowlevel.checkpoint_if_cancelled"),
            MethodName::FailAfter => write!(f, "trio.fail_after"),
            MethodName::FailAt => write!(f, "trio.fail_at"),
            MethodName::MoveOnAfter => write!(f, "trio.move_on_after"),
            MethodName::MoveOnAt => write!(f, "trio.move_on_at"),
            MethodName::OpenFile => write!(f, "trio.open_file"),
            MethodName::OpenProcess => write!(f, "trio.lowlevel.open_process"),
            MethodName::OpenSslOverTcpListeners => write!(f, "trio.open_ssl_over_tcp_listeners"),
            MethodName::OpenSslOverTcpStream => write!(f, "trio.open_ssl_over_tcp_stream"),
            MethodName::OpenTcpListeners => write!(f, "trio.open_tcp_listeners"),
            MethodName::OpenTcpStream => write!(f, "trio.open_tcp_stream"),
            MethodName::OpenUnixSocket => write!(f, "trio.open_unix_socket"),
            MethodName::PermanentlyDetachCoroutineObject => {
                write!(f, "trio.lowlevel.permanently_detach_coroutine_object")
            }
            MethodName::ReattachDetachedCoroutineObject => {
                write!(f, "trio.lowlevel.reattach_detached_coroutine_object")
            }
            MethodName::RunProcess => write!(f, "trio.run_process"),
            MethodName::ServeListeners => write!(f, "trio.serve_listeners"),
            MethodName::ServeSslOverTcp => write!(f, "trio.serve_ssl_over_tcp"),
            MethodName::ServeTcp => write!(f, "trio.serve_tcp"),
            MethodName::Sleep => write!(f, "trio.sleep"),
            MethodName::SleepForever => write!(f, "trio.sleep_forever"),
            MethodName::TemporarilyDetachCoroutineObject => {
                write!(f, "trio.lowlevel.temporarily_detach_coroutine_object")
            }
            MethodName::WaitReadable => write!(f, "trio.lowlevel.wait_readable"),
            MethodName::WaitTaskRescheduled => write!(f, "trio.lowlevel.wait_task_rescheduled"),
            MethodName::WaitWritable => write!(f, "trio.lowlevel.wait_writable"),
        }
    }
}
