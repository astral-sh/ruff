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
            AsyncModule::AnyIo => write!(f, "asyncio"),
            AsyncModule::AsyncIo => write!(f, "anyio"),
            AsyncModule::Trio => write!(f, "trio"),
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
            MethodName::TrioAcloseForcefully
                | MethodName::TrioCancelShieldedCheckpoint
                | MethodName::TrioCheckpoint
                | MethodName::TrioCheckpointIfCancelled
                | MethodName::TrioOpenFile
                | MethodName::TrioOpenProcess
                | MethodName::TrioOpenSslOverTcpListeners
                | MethodName::TrioOpenSslOverTcpStream
                | MethodName::TrioOpenTcpListeners
                | MethodName::TrioOpenTcpStream
                | MethodName::TrioOpenUnixSocket
                | MethodName::TrioPermanentlyDetachCoroutineObject
                | MethodName::TrioReattachDetachedCoroutineObject
                | MethodName::TrioRunProcess
                | MethodName::TrioServeListeners
                | MethodName::TrioServeSslOverTcp
                | MethodName::TrioServeTcp
                | MethodName::TrioSleep
                | MethodName::TrioSleepForever
                | MethodName::TrioTemporarilyDetachCoroutineObject
                | MethodName::TrioWaitReadable
                | MethodName::TrioWaitTaskRescheduled
                | MethodName::TrioWaitWritable
        )
    }

    /// Returns `true` if the method a timeout context manager.
    pub(super) fn is_timeout_context(self) -> bool {
        matches!(
            self,
            MethodName::AsyncIoTimeout
                | MethodName::AsyncIoTimeoutAt
                | MethodName::AnyIoMoveOnAfter
                | MethodName::AnyIoFailAfter
                | MethodName::AnyIoCancelScope
                | MethodName::TrioMoveOnAfter
                | MethodName::TrioMoveOnAt
                | MethodName::TrioFailAfter
                | MethodName::TrioFailAt
                | MethodName::TrioCancelScope
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
            MethodName::AsyncIoTimeout => write!(f, "asyncio.timeout"),
            MethodName::AsyncIoTimeoutAt => write!(f, "asyncio.timeout_at"),
            MethodName::AnyIoMoveOnAfter => write!(f, "anyio.move_on_after"),
            MethodName::AnyIoFailAfter => write!(f, "anyio.fail_after"),
            MethodName::AnyIoCancelScope => write!(f, "anyio.CancelScope"),
            MethodName::TrioAcloseForcefully => write!(f, "trio.aclose_forcefully"),
            MethodName::TrioCancelScope => write!(f, "trio.CancelScope"),
            MethodName::TrioCancelShieldedCheckpoint => {
                write!(f, "trio.lowlevel.cancel_shielded_checkpoint")
            }
            MethodName::TrioCheckpoint => write!(f, "trio.lowlevel.checkpoint"),
            MethodName::TrioCheckpointIfCancelled => {
                write!(f, "trio.lowlevel.checkpoint_if_cancelled")
            }
            MethodName::TrioFailAfter => write!(f, "trio.fail_after"),
            MethodName::TrioFailAt => write!(f, "trio.fail_at"),
            MethodName::TrioMoveOnAfter => write!(f, "trio.move_on_after"),
            MethodName::TrioMoveOnAt => write!(f, "trio.move_on_at"),
            MethodName::TrioOpenFile => write!(f, "trio.open_file"),
            MethodName::TrioOpenProcess => write!(f, "trio.lowlevel.open_process"),
            MethodName::TrioOpenSslOverTcpListeners => {
                write!(f, "trio.open_ssl_over_tcp_listeners")
            }
            MethodName::TrioOpenSslOverTcpStream => write!(f, "trio.open_ssl_over_tcp_stream"),
            MethodName::TrioOpenTcpListeners => write!(f, "trio.open_tcp_listeners"),
            MethodName::TrioOpenTcpStream => write!(f, "trio.open_tcp_stream"),
            MethodName::TrioOpenUnixSocket => write!(f, "trio.open_unix_socket"),
            MethodName::TrioPermanentlyDetachCoroutineObject => {
                write!(f, "trio.lowlevel.permanently_detach_coroutine_object")
            }
            MethodName::TrioReattachDetachedCoroutineObject => {
                write!(f, "trio.lowlevel.reattach_detached_coroutine_object")
            }
            MethodName::TrioRunProcess => write!(f, "trio.run_process"),
            MethodName::TrioServeListeners => write!(f, "trio.serve_listeners"),
            MethodName::TrioServeSslOverTcp => write!(f, "trio.serve_ssl_over_tcp"),
            MethodName::TrioServeTcp => write!(f, "trio.serve_tcp"),
            MethodName::TrioSleep => write!(f, "trio.sleep"),
            MethodName::TrioSleepForever => write!(f, "trio.sleep_forever"),
            MethodName::TrioTemporarilyDetachCoroutineObject => {
                write!(f, "trio.lowlevel.temporarily_detach_coroutine_object")
            }
            MethodName::TrioWaitReadable => write!(f, "trio.lowlevel.wait_readable"),
            MethodName::TrioWaitTaskRescheduled => write!(f, "trio.lowlevel.wait_task_rescheduled"),
            MethodName::TrioWaitWritable => write!(f, "trio.lowlevel.wait_writable"),
        }
    }
}
