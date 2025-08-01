import pickle
import sys
from _pickle import _ReducedType
from _typeshed import HasFileno, SupportsWrite, Unused
from abc import ABCMeta
from builtins import type as Type  # alias to avoid name clash
from collections.abc import Callable
from copyreg import _DispatchTableType
from multiprocessing import connection
from socket import socket
from typing import Any, Final

if sys.platform == "win32":
    __all__ = ["send_handle", "recv_handle", "ForkingPickler", "register", "dump", "DupHandle", "duplicate", "steal_handle"]
else:
    __all__ = ["send_handle", "recv_handle", "ForkingPickler", "register", "dump", "DupFd", "sendfds", "recvfds"]

HAVE_SEND_HANDLE: Final[bool]

class ForkingPickler(pickle.Pickler):
    """Pickler subclass used by multiprocessing."""

    dispatch_table: _DispatchTableType
    def __init__(self, file: SupportsWrite[bytes], protocol: int | None = ...) -> None: ...
    @classmethod
    def register(cls, type: Type, reduce: Callable[[Any], _ReducedType]) -> None:
        """Register a reduce function for a type."""

    @classmethod
    def dumps(cls, obj: Any, protocol: int | None = None) -> memoryview: ...
    loads = pickle.loads

register = ForkingPickler.register

def dump(obj: Any, file: SupportsWrite[bytes], protocol: int | None = None) -> None:
    """Replacement for pickle.dump() using ForkingPickler."""

if sys.platform == "win32":
    def duplicate(
        handle: int, target_process: int | None = None, inheritable: bool = False, *, source_process: int | None = None
    ) -> int:
        """Duplicate a handle.  (target_process is a handle not a pid!)"""

    def steal_handle(source_pid: int, handle: int) -> int:
        """Steal a handle from process identified by source_pid."""

    def send_handle(conn: connection.PipeConnection[DupHandle, Any], handle: int, destination_pid: int) -> None:
        """Send a handle over a local connection."""

    def recv_handle(conn: connection.PipeConnection[Any, DupHandle]) -> int:
        """Receive a handle over a local connection."""

    class DupHandle:
        """Picklable wrapper for a handle."""

        def __init__(self, handle: int, access: int, pid: int | None = None) -> None: ...
        def detach(self) -> int:
            """Get the handle.  This should only be called once."""

else:
    if sys.version_info < (3, 14):
        ACKNOWLEDGE: Final[bool]

    def recvfds(sock: socket, size: int) -> list[int]:
        """Receive an array of fds over an AF_UNIX socket."""

    def send_handle(conn: HasFileno, handle: int, destination_pid: Unused) -> None:
        """Send a handle over a local connection."""

    def recv_handle(conn: HasFileno) -> int:
        """Receive a handle over a local connection."""

    def sendfds(sock: socket, fds: list[int]) -> None:
        """Send an array of fds over an AF_UNIX socket."""

    def DupFd(fd: int) -> Any:  # Return type is really hard to get right
        """Return a wrapper for an fd."""

# These aliases are to work around pyright complaints.
# Pyright doesn't like it when a class object is defined as an alias
# of a global object with the same name.
_ForkingPickler = ForkingPickler
_register = register
_dump = dump
_send_handle = send_handle
_recv_handle = recv_handle

if sys.platform == "win32":
    _steal_handle = steal_handle
    _duplicate = duplicate
    _DupHandle = DupHandle
else:
    _sendfds = sendfds
    _recvfds = recvfds
    _DupFd = DupFd

class AbstractReducer(metaclass=ABCMeta):
    """Abstract base class for use in implementing a Reduction class
    suitable for use in replacing the standard reduction mechanism
    used in multiprocessing.
    """

    ForkingPickler = _ForkingPickler
    register = _register
    dump = _dump
    send_handle = _send_handle
    recv_handle = _recv_handle
    if sys.platform == "win32":
        steal_handle = _steal_handle
        duplicate = _duplicate
        DupHandle = _DupHandle
    else:
        sendfds = _sendfds
        recvfds = _recvfds
        DupFd = _DupFd

    def __init__(self, *args: Unused) -> None: ...
