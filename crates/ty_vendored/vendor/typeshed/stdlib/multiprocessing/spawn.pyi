from collections.abc import Mapping, Sequence
from types import ModuleType
from typing import Any, Final

__all__ = [
    "_main",
    "freeze_support",
    "set_executable",
    "get_executable",
    "get_preparation_data",
    "get_command_line",
    "import_main_path",
]

WINEXE: Final[bool]
WINSERVICE: Final[bool]

def set_executable(exe: str) -> None: ...
def get_executable() -> str: ...
def is_forking(argv: Sequence[str]) -> bool:
    """
    Return whether commandline indicates we are forking
    """

def freeze_support() -> None:
    """
    Run code for process object if this in not the main process
    """

def get_command_line(**kwds: Any) -> list[str]:
    """
    Returns prefix of command line used for spawning a child process
    """

def spawn_main(pipe_handle: int, parent_pid: int | None = None, tracker_fd: int | None = None) -> None:
    """
    Run code specified by data received over pipe
    """

# undocumented
def _main(fd: int, parent_sentinel: int) -> int: ...
def get_preparation_data(name: str) -> dict[str, Any]:
    """
    Return info about parent needed by child to unpickle process object
    """

old_main_modules: list[ModuleType]

def prepare(data: Mapping[str, Any]) -> None:
    """
    Try to get current process ready to unpickle process object
    """

def import_main_path(main_path: str) -> None:
    """
    Set sys.modules['__main__'] to module at main_path
    """
