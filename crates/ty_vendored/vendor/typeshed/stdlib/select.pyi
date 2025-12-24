"""This module supports asynchronous I/O on multiple file descriptors.

*** IMPORTANT NOTICE ***
On Windows, only sockets are supported; on Unix, all file descriptors.
"""

import sys
from _typeshed import FileDescriptorLike
from collections.abc import Iterable
from types import TracebackType
from typing import Any, ClassVar, Final, TypeVar, final
from typing_extensions import Never, Self

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
        """Returns a polling object.

        This object supports registering and unregistering file descriptors, and then
        polling them for I/O events.
        """

        # default value is select.POLLIN | select.POLLPRI | select.POLLOUT
        def register(self, fd: FileDescriptorLike, eventmask: int = 7, /) -> None: ...
        def modify(self, fd: FileDescriptorLike, eventmask: int, /) -> None: ...
        def unregister(self, fd: FileDescriptorLike, /) -> None: ...
        def poll(self, timeout: float | None = None, /) -> list[tuple[int, int]]: ...

_R = TypeVar("_R", default=Never)
_W = TypeVar("_W", default=Never)
_X = TypeVar("_X", default=Never)

def select(
    rlist: Iterable[_R], wlist: Iterable[_W], xlist: Iterable[_X], timeout: float | None = None, /
) -> tuple[list[_R], list[_W], list[_X]]:
    """Wait until one or more file descriptors are ready for some kind of I/O.

    The first three arguments are iterables of file descriptors to be waited for:
    rlist -- wait until ready for reading
    wlist -- wait until ready for writing
    xlist -- wait for an "exceptional condition"
    If only one kind of condition is required, pass [] for the other lists.

    A file descriptor is either a socket or file object, or a small integer
    gotten from a fileno() method call on one of those.

    The optional 4th argument specifies a timeout in seconds; it may be
    a floating-point number to specify fractions of seconds.  If it is absent
    or None, the call will never time out.

    The return value is a tuple of three lists corresponding to the first three
    arguments; each contains the subset of the corresponding file descriptors
    that are ready.

    *** IMPORTANT NOTICE ***
    On Windows, only sockets are supported; on Unix, all file
    descriptors can be used.
    """

error = OSError

if sys.platform != "linux" and sys.platform != "win32":
    # BSD only
    @final
    class kevent:
        """kevent(ident, filter=KQ_FILTER_READ, flags=KQ_EV_ADD, fflags=0, data=0, udata=0)

        This object is the equivalent of the struct kevent for the C API.

        See the kqueue manpage for more detailed information about the meaning
        of the arguments.

        One minor note: while you might hope that udata could store a
        reference to a python object, it cannot, because it is impossible to
        keep a proper reference count of the object once it's passed into the
        kernel. Therefore, I have restricted it to only storing an integer.  I
        recommend ignoring it and simply using the 'ident' field to key off
        of. You could also set up a dictionary on the python side to store a
        udata->object mapping.
        """

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
        """Kqueue syscall wrapper.

        For example, to start watching a socket for input:
        >>> kq = kqueue()
        >>> sock = socket()
        >>> sock.connect((host, port))
        >>> kq.control([kevent(sock, KQ_FILTER_WRITE, KQ_EV_ADD)], 0)

        To wait one second for it to become writeable:
        >>> kq.control(None, 1, 1000)

        To stop listening:
        >>> kq.control([kevent(sock, KQ_FILTER_WRITE, KQ_EV_DELETE)], 0)
        """

        closed: bool
        def __init__(self) -> None: ...
        def close(self) -> None:
            """Close the kqueue control file descriptor.

            Further operations on the kqueue object will raise an exception.
            """

        def control(self, changelist: Iterable[kevent] | None, maxevents: int, timeout: float | None = None, /) -> list[kevent]:
            """Calls the kernel kevent function.

            changelist
              Must be an iterable of kevent objects describing the changes to be made
              to the kernel's watch list or None.
            maxevents
              The maximum number of events that the kernel will return.
            timeout
              The maximum time to wait in seconds, or else None to wait forever.
              This accepts floats for smaller timeouts, too.
            """

        def fileno(self) -> int:
            """Return the kqueue control file descriptor."""

        @classmethod
        def fromfd(cls, fd: FileDescriptorLike, /) -> kqueue:
            """Create a kqueue object from a given control fd."""

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
        """select.epoll(sizehint=-1, flags=0)

        Returns an epolling object

        sizehint must be a positive integer or -1 for the default size. The
        sizehint is used to optimize internal data structures. It doesn't limit
        the maximum number of monitored events.
        """

        def __new__(self, sizehint: int = ..., flags: int = ...) -> Self: ...
        def __enter__(self) -> Self: ...
        def __exit__(
            self,
            exc_type: type[BaseException] | None = None,
            exc_value: BaseException | None = ...,
            exc_tb: TracebackType | None = None,
            /,
        ) -> None: ...
        def close(self) -> None:
            """Close the epoll control file descriptor.

            Further operations on the epoll object will raise an exception.
            """
        closed: bool
        def fileno(self) -> int:
            """Return the epoll control file descriptor."""

        def register(self, fd: FileDescriptorLike, eventmask: int = ...) -> None:
            """Registers a new fd or raises an OSError if the fd is already registered.

              fd
                the target file descriptor of the operation
              eventmask
                a bit set composed of the various EPOLL constants

            The epoll interface supports all file descriptors that support poll.
            """

        def modify(self, fd: FileDescriptorLike, eventmask: int) -> None:
            """Modify event mask for a registered file descriptor.

            fd
              the target file descriptor of the operation
            eventmask
              a bit set composed of the various EPOLL constants
            """

        def unregister(self, fd: FileDescriptorLike) -> None:
            """Remove a registered file descriptor from the epoll object.

            fd
              the target file descriptor of the operation
            """

        def poll(self, timeout: float | None = None, maxevents: int = -1) -> list[tuple[int, int]]:
            """Wait for events on the epoll file descriptor.

              timeout
                the maximum time to wait in seconds (as float);
                a timeout of None or -1 makes poll wait indefinitely
              maxevents
                the maximum number of events returned; -1 means no limit

            Returns a list containing any descriptors that have events to report,
            as a list of (fd, events) 2-tuples.
            """

        @classmethod
        def fromfd(cls, fd: FileDescriptorLike, /) -> epoll:
            """Create an epoll object from a given control fd."""

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
