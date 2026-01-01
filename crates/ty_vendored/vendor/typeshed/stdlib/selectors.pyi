"""Selectors module.

This module allows high-level and efficient I/O multiplexing, built upon the
`select` module primitives.
"""

import sys
from _typeshed import FileDescriptor, FileDescriptorLike, Unused
from abc import ABCMeta, abstractmethod
from collections.abc import Mapping
from typing import Any, Final, NamedTuple
from typing_extensions import Self, TypeAlias

_EventMask: TypeAlias = int

EVENT_READ: Final = 1
EVENT_WRITE: Final = 2

class SelectorKey(NamedTuple):
    """SelectorKey(fileobj, fd, events, data)

    Object used to associate a file object to its backing
    file descriptor, selected event mask, and attached data.
    """

    fileobj: FileDescriptorLike
    fd: FileDescriptor
    events: _EventMask
    data: Any

class BaseSelector(metaclass=ABCMeta):
    """Selector abstract base class.

    A selector supports registering file objects to be monitored for specific
    I/O events.

    A file object is a file descriptor or any object with a `fileno()` method.
    An arbitrary object can be attached to the file object, which can be used
    for example to store context information, a callback, etc.

    A selector can use various implementations (select(), poll(), epoll()...)
    depending on the platform. The default `Selector` class uses the most
    efficient implementation on the current platform.
    """

    @abstractmethod
    def register(self, fileobj: FileDescriptorLike, events: _EventMask, data: Any = None) -> SelectorKey:
        """Register a file object.

        Parameters:
        fileobj -- file object or file descriptor
        events  -- events to monitor (bitwise mask of EVENT_READ|EVENT_WRITE)
        data    -- attached data

        Returns:
        SelectorKey instance

        Raises:
        ValueError if events is invalid
        KeyError if fileobj is already registered
        OSError if fileobj is closed or otherwise is unacceptable to
                the underlying system call (if a system call is made)

        Note:
        OSError may or may not be raised
        """

    @abstractmethod
    def unregister(self, fileobj: FileDescriptorLike) -> SelectorKey:
        """Unregister a file object.

        Parameters:
        fileobj -- file object or file descriptor

        Returns:
        SelectorKey instance

        Raises:
        KeyError if fileobj is not registered

        Note:
        If fileobj is registered but has since been closed this does
        *not* raise OSError (even if the wrapped syscall does)
        """

    def modify(self, fileobj: FileDescriptorLike, events: _EventMask, data: Any = None) -> SelectorKey:
        """Change a registered file object monitored events or attached data.

        Parameters:
        fileobj -- file object or file descriptor
        events  -- events to monitor (bitwise mask of EVENT_READ|EVENT_WRITE)
        data    -- attached data

        Returns:
        SelectorKey instance

        Raises:
        Anything that unregister() or register() raises
        """

    @abstractmethod
    def select(self, timeout: float | None = None) -> list[tuple[SelectorKey, _EventMask]]:
        """Perform the actual selection, until some monitored file objects are
        ready or a timeout expires.

        Parameters:
        timeout -- if timeout > 0, this specifies the maximum wait time, in
                   seconds
                   if timeout <= 0, the select() call won't block, and will
                   report the currently ready file objects
                   if timeout is None, select() will block until a monitored
                   file object becomes ready

        Returns:
        list of (key, events) for ready file objects
        `events` is a bitwise mask of EVENT_READ|EVENT_WRITE
        """

    def close(self) -> None:
        """Close the selector.

        This must be called to make sure that any underlying resource is freed.
        """

    def get_key(self, fileobj: FileDescriptorLike) -> SelectorKey:
        """Return the key associated to a registered file object.

        Returns:
        SelectorKey for this file object
        """

    @abstractmethod
    def get_map(self) -> Mapping[FileDescriptorLike, SelectorKey]:
        """Return a mapping of file objects to selector keys."""

    def __enter__(self) -> Self: ...
    def __exit__(self, *args: Unused) -> None: ...

class _BaseSelectorImpl(BaseSelector, metaclass=ABCMeta):
    """Base selector implementation."""

    def register(self, fileobj: FileDescriptorLike, events: _EventMask, data: Any = None) -> SelectorKey: ...
    def unregister(self, fileobj: FileDescriptorLike) -> SelectorKey: ...
    def modify(self, fileobj: FileDescriptorLike, events: _EventMask, data: Any = None) -> SelectorKey: ...
    def get_map(self) -> Mapping[FileDescriptorLike, SelectorKey]: ...

class SelectSelector(_BaseSelectorImpl):
    """Select-based selector."""

    def select(self, timeout: float | None = None) -> list[tuple[SelectorKey, _EventMask]]: ...

class _PollLikeSelector(_BaseSelectorImpl):
    """Base class shared between poll, epoll and devpoll selectors."""

    def select(self, timeout: float | None = None) -> list[tuple[SelectorKey, _EventMask]]: ...

if sys.platform != "win32":
    class PollSelector(_PollLikeSelector):
        """Poll-based selector."""

if sys.platform == "linux":
    class EpollSelector(_PollLikeSelector):
        """Epoll-based selector."""

        def fileno(self) -> int: ...

if sys.platform != "linux" and sys.platform != "darwin" and sys.platform != "win32":
    # Solaris only
    class DevpollSelector(_PollLikeSelector):
        def fileno(self) -> int: ...

if sys.platform != "win32" and sys.platform != "linux":
    class KqueueSelector(_BaseSelectorImpl):
        """Kqueue-based selector."""

        def fileno(self) -> int: ...
        def select(self, timeout: float | None = None) -> list[tuple[SelectorKey, _EventMask]]: ...

# Not a real class at runtime, it is just a conditional alias to other real selectors.
# The runtime logic is more fine-grained than a `sys.platform` check;
# not really expressible in the stubs
class DefaultSelector(_BaseSelectorImpl):
    """Epoll-based selector."""

    def select(self, timeout: float | None = None) -> list[tuple[SelectorKey, _EventMask]]: ...
    if sys.platform != "win32":
        def fileno(self) -> int: ...
