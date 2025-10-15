import sys
from _typeshed import FileDescriptorLike
from collections.abc import Iterable
from types import TracebackType
from typing import Any, ClassVar, Final, final
from typing_extensions import Self

if sys.platform != "win32":
    PIPE_BUF: Final[int]
    POLLERR: Final[int]
    POLLHUP: Final[int]
    POLLIN: Final[int]
    if sys.platform == "linux":
        POLLMSG: Final[int]
    POLLNVAL: Final[int]
    POLLOUT: Final[int]
    POLLPRI: Final[int]
    POLLRDBAND: Final[int]
    if sys.platform == "linux":
        POLLRDHUP: Final[int]
    POLLRDNORM: Final[int]
    POLLWRBAND: Final[int]
    POLLWRNORM: Final[int]

    # This is actually a function that returns an instance of a class.
    # The class is not accessible directly, and also calls itself select.poll.
    class poll:
        # default value is select.POLLIN | select.POLLPRI | select.POLLOUT
        def register(self, fd: FileDescriptorLike, eventmask: int = 7, /) -> None: ...
        def modify(self, fd: FileDescriptorLike, eventmask: int, /) -> None: ...
        def unregister(self, fd: FileDescriptorLike, /) -> None: ...
        def poll(self, timeout: float | None = None, /) -> list[tuple[int, int]]: ...

def select(
    rlist: Iterable[Any], wlist: Iterable[Any], xlist: Iterable[Any], timeout: float | None = None, /
) -> tuple[list[Any], list[Any], list[Any]]: ...

error = OSError

if sys.platform != "linux" and sys.platform != "win32":
    # BSD only
    @final
    class kevent:
        data: Any
        fflags: int
        filter: int
        flags: int
        ident: int
        udata: Any
        def __init__(
            self,
            ident: FileDescriptorLike,
            filter: int = ...,
            flags: int = ...,
            fflags: int = ...,
            data: Any = ...,
            udata: Any = ...,
        ) -> None: ...
        __hash__: ClassVar[None]  # type: ignore[assignment]

    # BSD only
    @final
    class kqueue:
        closed: bool
        def __init__(self) -> None: ...
        def close(self) -> None: ...
        def control(
            self, changelist: Iterable[kevent] | None, maxevents: int, timeout: float | None = None, /
        ) -> list[kevent]: ...
        def fileno(self) -> int: ...
        @classmethod
        def fromfd(cls, fd: FileDescriptorLike, /) -> kqueue: ...

    KQ_EV_ADD: Final[int]
    KQ_EV_CLEAR: Final[int]
    KQ_EV_DELETE: Final[int]
    KQ_EV_DISABLE: Final[int]
    KQ_EV_ENABLE: Final[int]
    KQ_EV_EOF: Final[int]
    KQ_EV_ERROR: Final[int]
    KQ_EV_FLAG1: Final[int]
    KQ_EV_ONESHOT: Final[int]
    KQ_EV_SYSFLAGS: Final[int]
    KQ_FILTER_AIO: Final[int]
    if sys.platform != "darwin":
        KQ_FILTER_NETDEV: Final[int]
    KQ_FILTER_PROC: Final[int]
    KQ_FILTER_READ: Final[int]
    KQ_FILTER_SIGNAL: Final[int]
    KQ_FILTER_TIMER: Final[int]
    KQ_FILTER_VNODE: Final[int]
    KQ_FILTER_WRITE: Final[int]
    KQ_NOTE_ATTRIB: Final[int]
    KQ_NOTE_CHILD: Final[int]
    KQ_NOTE_DELETE: Final[int]
    KQ_NOTE_EXEC: Final[int]
    KQ_NOTE_EXIT: Final[int]
    KQ_NOTE_EXTEND: Final[int]
    KQ_NOTE_FORK: Final[int]
    KQ_NOTE_LINK: Final[int]
    if sys.platform != "darwin":
        KQ_NOTE_LINKDOWN: Final[int]
        KQ_NOTE_LINKINV: Final[int]
        KQ_NOTE_LINKUP: Final[int]
    KQ_NOTE_LOWAT: Final[int]
    KQ_NOTE_PCTRLMASK: Final[int]
    KQ_NOTE_PDATAMASK: Final[int]
    KQ_NOTE_RENAME: Final[int]
    KQ_NOTE_REVOKE: Final[int]
    KQ_NOTE_TRACK: Final[int]
    KQ_NOTE_TRACKERR: Final[int]
    KQ_NOTE_WRITE: Final[int]

if sys.platform == "linux":
    @final
    class epoll:
        def __new__(self, sizehint: int = ..., flags: int = ...) -> Self: ...
        def __enter__(self) -> Self: ...
        def __exit__(
            self,
            exc_type: type[BaseException] | None = None,
            exc_value: BaseException | None = ...,
            exc_tb: TracebackType | None = None,
            /,
        ) -> None: ...
        def close(self) -> None: ...
        closed: bool
        def fileno(self) -> int: ...
        def register(self, fd: FileDescriptorLike, eventmask: int = ...) -> None: ...
        def modify(self, fd: FileDescriptorLike, eventmask: int) -> None: ...
        def unregister(self, fd: FileDescriptorLike) -> None: ...
        def poll(self, timeout: float | None = None, maxevents: int = -1) -> list[tuple[int, int]]: ...
        @classmethod
        def fromfd(cls, fd: FileDescriptorLike, /) -> epoll: ...

    EPOLLERR: Final[int]
    EPOLLEXCLUSIVE: Final[int]
    EPOLLET: Final[int]
    EPOLLHUP: Final[int]
    EPOLLIN: Final[int]
    EPOLLMSG: Final[int]
    EPOLLONESHOT: Final[int]
    EPOLLOUT: Final[int]
    EPOLLPRI: Final[int]
    EPOLLRDBAND: Final[int]
    EPOLLRDHUP: Final[int]
    EPOLLRDNORM: Final[int]
    EPOLLWRBAND: Final[int]
    EPOLLWRNORM: Final[int]
    EPOLL_CLOEXEC: Final[int]
    if sys.version_info >= (3, 14):
        EPOLLWAKEUP: Final[int]

if sys.platform != "linux" and sys.platform != "darwin" and sys.platform != "win32":
    # Solaris only
    class devpoll:
        def close(self) -> None: ...
        closed: bool
        def fileno(self) -> int: ...
        def register(self, fd: FileDescriptorLike, eventmask: int = ...) -> None: ...
        def modify(self, fd: FileDescriptorLike, eventmask: int = ...) -> None: ...
        def unregister(self, fd: FileDescriptorLike) -> None: ...
        def poll(self, timeout: float | None = ...) -> list[tuple[int, int]]: ...
