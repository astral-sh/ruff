import sys
from _typeshed import FileDescriptorLike, Unused
from collections.abc import Sequence
from struct import Struct
from typing import Any, Final

__all__ = ["ensure_running", "get_inherited_fds", "connect_to_new_process", "set_forkserver_preload"]

MAXFDS_TO_SEND: Final = 256
SIGNED_STRUCT: Final[Struct]

class ForkServer:
    def set_forkserver_preload(self, modules_names: list[str]) -> None: ...
    def get_inherited_fds(self) -> list[int] | None: ...
    def connect_to_new_process(self, fds: Sequence[int]) -> tuple[int, int]: ...
    def ensure_running(self) -> None: ...

if sys.version_info >= (3, 14):
    def main(
        listener_fd: int | None,
        alive_r: FileDescriptorLike,
        preload: Sequence[str],
        main_path: str | None = None,
        sys_path: list[str] | None = None,
        *,
        authkey_r: int | None = None,
    ) -> None: ...

else:
    def main(
        listener_fd: int | None,
        alive_r: FileDescriptorLike,
        preload: Sequence[str],
        main_path: str | None = None,
        sys_path: Unused = None,
    ) -> None: ...

def read_signed(fd: int) -> Any: ...
def write_signed(fd: int, n: int) -> None: ...

_forkserver: ForkServer
ensure_running = _forkserver.ensure_running
get_inherited_fds = _forkserver.get_inherited_fds
connect_to_new_process = _forkserver.connect_to_new_process
set_forkserver_preload = _forkserver.set_forkserver_preload
