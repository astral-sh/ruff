import sys
from _typeshed import FileDescriptorLike, Unused
from collections.abc import Sequence
from struct import Struct
from typing import Any, Final

__all__ = ["ensure_running", "get_inherited_fds", "connect_to_new_process", "set_forkserver_preload"]

MAXFDS_TO_SEND: Final = 256
SIGNED_STRUCT: Final[Struct]

class ForkServer:
    def set_forkserver_preload(self, modules_names: list[str]) -> None:
        """Set list of module names to try to load in forkserver process."""

    def get_inherited_fds(self) -> list[int] | None:
        """Return list of fds inherited from parent process.

        This returns None if the current process was not started by fork
        server.
        """

    def connect_to_new_process(self, fds: Sequence[int]) -> tuple[int, int]:
        """Request forkserver to create a child process.

        Returns a pair of fds (status_r, data_w).  The calling process can read
        the child process's pid and (eventually) its returncode from status_r.
        The calling process should write to data_w the pickled preparation and
        process data.
        """

    def ensure_running(self) -> None:
        """Make sure that a fork server is running.

        This can be called from any process.  Note that usually a child
        process will just reuse the forkserver started by its parent, so
        ensure_running() will do nothing.
        """

if sys.version_info >= (3, 14):
    def main(
        listener_fd: int | None,
        alive_r: FileDescriptorLike,
        preload: Sequence[str],
        main_path: str | None = None,
        sys_path: list[str] | None = None,
        *,
        authkey_r: int | None = None,
    ) -> None:
        """Run forkserver."""

else:
    def main(
        listener_fd: int | None,
        alive_r: FileDescriptorLike,
        preload: Sequence[str],
        main_path: str | None = None,
        sys_path: Unused = None,
    ) -> None:
        """Run forkserver."""

def read_signed(fd: int) -> Any: ...
def write_signed(fd: int, n: int) -> None: ...

_forkserver: ForkServer
ensure_running = _forkserver.ensure_running
get_inherited_fds = _forkserver.get_inherited_fds
connect_to_new_process = _forkserver.connect_to_new_process
set_forkserver_preload = _forkserver.set_forkserver_preload
