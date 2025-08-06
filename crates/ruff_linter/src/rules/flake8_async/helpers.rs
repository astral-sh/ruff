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

impl AsyncModule {
    pub(super) fn try_from(qualified_name: &QualifiedName<'_>) -> Option<Self> {
        match qualified_name.segments() {
            ["asyncio", ..] => Some(Self::AsyncIo),
            ["anyio", ..] => Some(Self::AnyIo),
            ["trio", ..] => Some(Self::Trio),
            _ => None,
        }
    }
}

impl std::fmt::Display for AsyncModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AnyIo => write!(f, "anyio"),
            Self::AsyncIo => write!(f, "asyncio"),
            Self::Trio => write!(f, "trio"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum MethodName {
    AsyncIoTimeout,
    AsyncIoTimeoutAt,
    AnyIoMoveOnAfter,
    AnyIoFailAfter,
    AnyIoCancelScope,
    TrioAcloseForcefully,
    TrioCancelScope,
    TrioCancelShieldedCheckpoint,
    TrioCheckpoint,
    TrioCheckpointIfCancelled,
    TrioFailAfter,
    TrioFailAt,
    TrioMoveOnAfter,
    TrioMoveOnAt,
    TrioOpenFile,
    TrioOpenProcess,
    TrioOpenSslOverTcpListeners,
    TrioOpenSslOverTcpStream,
    TrioOpenTcpListeners,
    TrioOpenTcpStream,
    TrioOpenUnixSocket,
    TrioPermanentlyDetachCoroutineObject,
    TrioReattachDetachedCoroutineObject,
    TrioRunProcess,
    TrioServeListeners,
    TrioServeSslOverTcp,
    TrioServeTcp,
    TrioSleep,
    TrioSleepForever,
    TrioTemporarilyDetachCoroutineObject,
    TrioWaitReadable,
    TrioWaitTaskRescheduled,
    TrioWaitWritable,
}

impl MethodName {
    /// Returns `true` if the method is async, `false` if it is sync.
    pub(super) fn is_async(self) -> bool {
        matches!(
            self,
            Self::TrioAcloseForcefully
                | Self::TrioCancelShieldedCheckpoint
                | Self::TrioCheckpoint
                | Self::TrioCheckpointIfCancelled
                | Self::TrioOpenFile
                | Self::TrioOpenProcess
                | Self::TrioOpenSslOverTcpListeners
                | Self::TrioOpenSslOverTcpStream
                | Self::TrioOpenTcpListeners
                | Self::TrioOpenTcpStream
                | Self::TrioOpenUnixSocket
                | Self::TrioPermanentlyDetachCoroutineObject
                | Self::TrioReattachDetachedCoroutineObject
                | Self::TrioRunProcess
                | Self::TrioServeListeners
                | Self::TrioServeSslOverTcp
                | Self::TrioServeTcp
                | Self::TrioSleep
                | Self::TrioSleepForever
                | Self::TrioTemporarilyDetachCoroutineObject
                | Self::TrioWaitReadable
                | Self::TrioWaitTaskRescheduled
                | Self::TrioWaitWritable
        )
    }

    /// Returns `true` if the method a timeout context manager.
    pub(super) fn is_timeout_context(self) -> bool {
        matches!(
            self,
            Self::AsyncIoTimeout
                | Self::AsyncIoTimeoutAt
                | Self::AnyIoMoveOnAfter
                | Self::AnyIoFailAfter
                | Self::AnyIoCancelScope
                | Self::TrioMoveOnAfter
                | Self::TrioMoveOnAt
                | Self::TrioFailAfter
                | Self::TrioFailAt
                | Self::TrioCancelScope
        )
    }
}

impl MethodName {
    pub(super) fn try_from(qualified_name: &QualifiedName<'_>) -> Option<Self> {
        match qualified_name.segments() {
            ["asyncio", "timeout"] => Some(Self::AsyncIoTimeout),
            ["asyncio", "timeout_at"] => Some(Self::AsyncIoTimeoutAt),
            ["anyio", "move_on_after"] => Some(Self::AnyIoMoveOnAfter),
            ["anyio", "fail_after"] => Some(Self::AnyIoFailAfter),
            ["anyio", "CancelScope"] => Some(Self::AnyIoCancelScope),
            ["trio", "CancelScope"] => Some(Self::TrioCancelScope),
            ["trio", "aclose_forcefully"] => Some(Self::TrioAcloseForcefully),
            ["trio", "fail_after"] => Some(Self::TrioFailAfter),
            ["trio", "fail_at"] => Some(Self::TrioFailAt),
            ["trio", "lowlevel", "cancel_shielded_checkpoint"] => {
                Some(Self::TrioCancelShieldedCheckpoint)
            }
            ["trio", "lowlevel", "checkpoint"] => Some(Self::TrioCheckpoint),
            ["trio", "lowlevel", "checkpoint_if_cancelled"] => {
                Some(Self::TrioCheckpointIfCancelled)
            }
            ["trio", "lowlevel", "open_process"] => Some(Self::TrioOpenProcess),
            ["trio", "lowlevel", "permanently_detach_coroutine_object"] => {
                Some(Self::TrioPermanentlyDetachCoroutineObject)
            }
            ["trio", "lowlevel", "reattach_detached_coroutine_object"] => {
                Some(Self::TrioReattachDetachedCoroutineObject)
            }
            ["trio", "lowlevel", "temporarily_detach_coroutine_object"] => {
                Some(Self::TrioTemporarilyDetachCoroutineObject)
            }
            ["trio", "lowlevel", "wait_readable"] => Some(Self::TrioWaitReadable),
            ["trio", "lowlevel", "wait_task_rescheduled"] => Some(Self::TrioWaitTaskRescheduled),
            ["trio", "lowlevel", "wait_writable"] => Some(Self::TrioWaitWritable),
            ["trio", "move_on_after"] => Some(Self::TrioMoveOnAfter),
            ["trio", "move_on_at"] => Some(Self::TrioMoveOnAt),
            ["trio", "open_file"] => Some(Self::TrioOpenFile),
            ["trio", "open_ssl_over_tcp_listeners"] => Some(Self::TrioOpenSslOverTcpListeners),
            ["trio", "open_ssl_over_tcp_stream"] => Some(Self::TrioOpenSslOverTcpStream),
            ["trio", "open_tcp_listeners"] => Some(Self::TrioOpenTcpListeners),
            ["trio", "open_tcp_stream"] => Some(Self::TrioOpenTcpStream),
            ["trio", "open_unix_socket"] => Some(Self::TrioOpenUnixSocket),
            ["trio", "run_process"] => Some(Self::TrioRunProcess),
            ["trio", "serve_listeners"] => Some(Self::TrioServeListeners),
            ["trio", "serve_ssl_over_tcp"] => Some(Self::TrioServeSslOverTcp),
            ["trio", "serve_tcp"] => Some(Self::TrioServeTcp),
            ["trio", "sleep"] => Some(Self::TrioSleep),
            ["trio", "sleep_forever"] => Some(Self::TrioSleepForever),
            _ => None,
        }
    }
}

impl std::fmt::Display for MethodName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AsyncIoTimeout => write!(f, "asyncio.timeout"),
            Self::AsyncIoTimeoutAt => write!(f, "asyncio.timeout_at"),
            Self::AnyIoMoveOnAfter => write!(f, "anyio.move_on_after"),
            Self::AnyIoFailAfter => write!(f, "anyio.fail_after"),
            Self::AnyIoCancelScope => write!(f, "anyio.CancelScope"),
            Self::TrioAcloseForcefully => write!(f, "trio.aclose_forcefully"),
            Self::TrioCancelScope => write!(f, "trio.CancelScope"),
            Self::TrioCancelShieldedCheckpoint => {
                write!(f, "trio.lowlevel.cancel_shielded_checkpoint")
            }
            Self::TrioCheckpoint => write!(f, "trio.lowlevel.checkpoint"),
            Self::TrioCheckpointIfCancelled => {
                write!(f, "trio.lowlevel.checkpoint_if_cancelled")
            }
            Self::TrioFailAfter => write!(f, "trio.fail_after"),
            Self::TrioFailAt => write!(f, "trio.fail_at"),
            Self::TrioMoveOnAfter => write!(f, "trio.move_on_after"),
            Self::TrioMoveOnAt => write!(f, "trio.move_on_at"),
            Self::TrioOpenFile => write!(f, "trio.open_file"),
            Self::TrioOpenProcess => write!(f, "trio.lowlevel.open_process"),
            Self::TrioOpenSslOverTcpListeners => {
                write!(f, "trio.open_ssl_over_tcp_listeners")
            }
            Self::TrioOpenSslOverTcpStream => write!(f, "trio.open_ssl_over_tcp_stream"),
            Self::TrioOpenTcpListeners => write!(f, "trio.open_tcp_listeners"),
            Self::TrioOpenTcpStream => write!(f, "trio.open_tcp_stream"),
            Self::TrioOpenUnixSocket => write!(f, "trio.open_unix_socket"),
            Self::TrioPermanentlyDetachCoroutineObject => {
                write!(f, "trio.lowlevel.permanently_detach_coroutine_object")
            }
            Self::TrioReattachDetachedCoroutineObject => {
                write!(f, "trio.lowlevel.reattach_detached_coroutine_object")
            }
            Self::TrioRunProcess => write!(f, "trio.run_process"),
            Self::TrioServeListeners => write!(f, "trio.serve_listeners"),
            Self::TrioServeSslOverTcp => write!(f, "trio.serve_ssl_over_tcp"),
            Self::TrioServeTcp => write!(f, "trio.serve_tcp"),
            Self::TrioSleep => write!(f, "trio.sleep"),
            Self::TrioSleepForever => write!(f, "trio.sleep_forever"),
            Self::TrioTemporarilyDetachCoroutineObject => {
                write!(f, "trio.lowlevel.temporarily_detach_coroutine_object")
            }
            Self::TrioWaitReadable => write!(f, "trio.lowlevel.wait_readable"),
            Self::TrioWaitTaskRescheduled => write!(f, "trio.lowlevel.wait_task_rescheduled"),
            Self::TrioWaitWritable => write!(f, "trio.lowlevel.wait_writable"),
        }
    }
}
