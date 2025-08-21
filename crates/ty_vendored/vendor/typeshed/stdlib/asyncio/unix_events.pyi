import sys
import types
from _typeshed import StrPath
from abc import ABCMeta, abstractmethod
from collections.abc import Callable
from socket import socket
from typing import Literal
from typing_extensions import Self, TypeVarTuple, Unpack, deprecated

from . import events
from .base_events import Server, _ProtocolFactory, _SSLContext
from .selector_events import BaseSelectorEventLoop

_Ts = TypeVarTuple("_Ts")

# Keep asyncio.__all__ updated with any changes to __all__ here
if sys.platform != "win32":
    if sys.version_info >= (3, 14):
        __all__ = ("SelectorEventLoop", "EventLoop")
    elif sys.version_info >= (3, 13):
        # Adds EventLoop
        __all__ = (
            "SelectorEventLoop",
            "AbstractChildWatcher",
            "SafeChildWatcher",
            "FastChildWatcher",
            "PidfdChildWatcher",
            "MultiLoopChildWatcher",
            "ThreadedChildWatcher",
            "DefaultEventLoopPolicy",
            "EventLoop",
        )
    else:
        # adds PidfdChildWatcher
        __all__ = (
            "SelectorEventLoop",
            "AbstractChildWatcher",
            "SafeChildWatcher",
            "FastChildWatcher",
            "PidfdChildWatcher",
            "MultiLoopChildWatcher",
            "ThreadedChildWatcher",
            "DefaultEventLoopPolicy",
        )

# This is also technically not available on Win,
# but other parts of typeshed need this definition.
# So, it is special cased.
if sys.version_info < (3, 14):
    if sys.version_info >= (3, 12):
        @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
        class AbstractChildWatcher:
            @abstractmethod
            def add_child_handler(
                self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
            ) -> None: ...
            @abstractmethod
            def remove_child_handler(self, pid: int) -> bool: ...
            @abstractmethod
            def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...
            @abstractmethod
            def close(self) -> None: ...
            @abstractmethod
            def __enter__(self) -> Self: ...
            @abstractmethod
            def __exit__(
                self, typ: type[BaseException] | None, exc: BaseException | None, tb: types.TracebackType | None
            ) -> None: ...
            @abstractmethod
            def is_active(self) -> bool: ...

    else:
        class AbstractChildWatcher:
            @abstractmethod
            def add_child_handler(
                self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
            ) -> None: ...
            @abstractmethod
            def remove_child_handler(self, pid: int) -> bool: ...
            @abstractmethod
            def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...
            @abstractmethod
            def close(self) -> None: ...
            @abstractmethod
            def __enter__(self) -> Self: ...
            @abstractmethod
            def __exit__(
                self, typ: type[BaseException] | None, exc: BaseException | None, tb: types.TracebackType | None
            ) -> None: ...
            @abstractmethod
            def is_active(self) -> bool: ...

if sys.platform != "win32":
    if sys.version_info < (3, 14):
        if sys.version_info >= (3, 12):
            # Doesn't actually have ABCMeta metaclass at runtime, but mypy complains if we don't have it in the stub.
            # See discussion in #7412
            class BaseChildWatcher(AbstractChildWatcher, metaclass=ABCMeta):
                def close(self) -> None: ...
                def is_active(self) -> bool: ...
                def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...

            @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
            class SafeChildWatcher(BaseChildWatcher):
                def __enter__(self) -> Self: ...
                def __exit__(
                    self, a: type[BaseException] | None, b: BaseException | None, c: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...

            @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
            class FastChildWatcher(BaseChildWatcher):
                def __enter__(self) -> Self: ...
                def __exit__(
                    self, a: type[BaseException] | None, b: BaseException | None, c: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...

        else:
            # Doesn't actually have ABCMeta metaclass at runtime, but mypy complains if we don't have it in the stub.
            # See discussion in #7412
            class BaseChildWatcher(AbstractChildWatcher, metaclass=ABCMeta):
                def close(self) -> None: ...
                def is_active(self) -> bool: ...
                def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...

            class SafeChildWatcher(BaseChildWatcher):
                def __enter__(self) -> Self: ...
                def __exit__(
                    self, a: type[BaseException] | None, b: BaseException | None, c: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...

            class FastChildWatcher(BaseChildWatcher):
                def __enter__(self) -> Self: ...
                def __exit__(
                    self, a: type[BaseException] | None, b: BaseException | None, c: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...

    class _UnixSelectorEventLoop(BaseSelectorEventLoop):
        if sys.version_info >= (3, 13):
            async def create_unix_server(
                self,
                protocol_factory: _ProtocolFactory,
                path: StrPath | None = None,
                *,
                sock: socket | None = None,
                backlog: int = 100,
                ssl: _SSLContext = None,
                ssl_handshake_timeout: float | None = None,
                ssl_shutdown_timeout: float | None = None,
                start_serving: bool = True,
                cleanup_socket: bool = True,
            ) -> Server: ...

    if sys.version_info >= (3, 14):
        class _UnixDefaultEventLoopPolicy(events._BaseDefaultEventLoopPolicy): ...
    else:
        class _UnixDefaultEventLoopPolicy(events.BaseDefaultEventLoopPolicy):
            if sys.version_info >= (3, 12):
                @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
                def get_child_watcher(self) -> AbstractChildWatcher: ...
                @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
                def set_child_watcher(self, watcher: AbstractChildWatcher | None) -> None: ...
            else:
                def get_child_watcher(self) -> AbstractChildWatcher: ...
                def set_child_watcher(self, watcher: AbstractChildWatcher | None) -> None: ...

    SelectorEventLoop = _UnixSelectorEventLoop

    if sys.version_info >= (3, 14):
        _DefaultEventLoopPolicy = _UnixDefaultEventLoopPolicy
    else:
        DefaultEventLoopPolicy = _UnixDefaultEventLoopPolicy

    if sys.version_info >= (3, 13):
        EventLoop = SelectorEventLoop

    if sys.version_info < (3, 14):
        if sys.version_info >= (3, 12):
            @deprecated("Deprecated since Python 3.12; removed in Python 3.14.")
            class MultiLoopChildWatcher(AbstractChildWatcher):
                def is_active(self) -> bool: ...
                def close(self) -> None: ...
                def __enter__(self) -> Self: ...
                def __exit__(
                    self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...
                def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...

        else:
            class MultiLoopChildWatcher(AbstractChildWatcher):
                def is_active(self) -> bool: ...
                def close(self) -> None: ...
                def __enter__(self) -> Self: ...
                def __exit__(
                    self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: types.TracebackType | None
                ) -> None: ...
                def add_child_handler(
                    self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
                ) -> None: ...
                def remove_child_handler(self, pid: int) -> bool: ...
                def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...

    if sys.version_info < (3, 14):
        class ThreadedChildWatcher(AbstractChildWatcher):
            def is_active(self) -> Literal[True]: ...
            def close(self) -> None: ...
            def __enter__(self) -> Self: ...
            def __exit__(
                self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: types.TracebackType | None
            ) -> None: ...
            def __del__(self) -> None: ...
            def add_child_handler(
                self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
            ) -> None: ...
            def remove_child_handler(self, pid: int) -> bool: ...
            def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...

        class PidfdChildWatcher(AbstractChildWatcher):
            def __enter__(self) -> Self: ...
            def __exit__(
                self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: types.TracebackType | None
            ) -> None: ...
            def is_active(self) -> bool: ...
            def close(self) -> None: ...
            def attach_loop(self, loop: events.AbstractEventLoop | None) -> None: ...
            def add_child_handler(
                self, pid: int, callback: Callable[[int, int, Unpack[_Ts]], object], *args: Unpack[_Ts]
            ) -> None: ...
            def remove_child_handler(self, pid: int) -> bool: ...
