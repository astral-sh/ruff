import sys
from collections.abc import Callable, Iterable, Mapping
from typing import Any

__all__ = ["BaseProcess", "current_process", "active_children", "parent_process"]

class BaseProcess:
    """
    Process objects represent activity that is run in a separate process

    The class is analogous to `threading.Thread`
    """

    name: str
    daemon: bool
    authkey: bytes
    _identity: tuple[int, ...]  # undocumented
    def __init__(
        self,
        group: None = None,
        target: Callable[..., object] | None = None,
        name: str | None = None,
        args: Iterable[Any] = (),
        kwargs: Mapping[str, Any] = {},
        *,
        daemon: bool | None = None,
    ) -> None: ...
    def run(self) -> None:
        """
        Method to be run in sub-process; can be overridden in sub-class
        """

    def start(self) -> None:
        """
        Start child process
        """
    if sys.version_info >= (3, 14):
        def interrupt(self) -> None:
            """
            Terminate process; sends SIGINT signal
            """

    def terminate(self) -> None:
        """
        Terminate process; sends SIGTERM signal or uses TerminateProcess()
        """

    def kill(self) -> None:
        """
        Terminate process; sends SIGKILL signal or uses TerminateProcess()
        """

    def close(self) -> None:
        """
        Close the Process object.

        This method releases resources held by the Process object.  It is
        an error to call this method if the child process is still running.
        """

    def join(self, timeout: float | None = None) -> None:
        """
        Wait until child process terminates
        """

    def is_alive(self) -> bool:
        """
        Return whether process is alive
        """

    @property
    def exitcode(self) -> int | None:
        """
        Return exit code of process or `None` if it has yet to stop
        """

    @property
    def ident(self) -> int | None:
        """
        Return identifier (PID) of process or `None` if it has yet to start
        """

    @property
    def pid(self) -> int | None:
        """
        Return identifier (PID) of process or `None` if it has yet to start
        """

    @property
    def sentinel(self) -> int:
        """
        Return a file descriptor (Unix) or handle (Windows) suitable for
        waiting for process termination.
        """

def current_process() -> BaseProcess:
    """
    Return process object representing the current process
    """

def active_children() -> list[BaseProcess]:
    """
    Return list of process objects corresponding to live child processes
    """

def parent_process() -> BaseProcess | None:
    """
    Return process object representing the parent process
    """
