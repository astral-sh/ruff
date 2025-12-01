"""Subprocesses with accessible I/O streams

This module allows you to spawn processes, connect to their
input/output/error pipes, and obtain their return codes.

For a complete description of this module see the Python documentation.

Main API
========
run(...): Runs a command, waits for it to complete, then returns a
          CompletedProcess instance.
Popen(...): A class for flexibly executing a command in a new process

Constants
---------
DEVNULL: Special value that indicates that os.devnull should be used
PIPE:    Special value that indicates a pipe should be created
STDOUT:  Special value that indicates that stderr should go to stdout


Older API
=========
call(...): Runs a command, waits for it to complete, then returns
    the return code.
check_call(...): Same as call() but raises CalledProcessError()
    if return code is not 0
check_output(...): Same as check_call() but returns the contents of
    stdout instead of a return code
getoutput(...): Runs a command in the shell, waits for it to complete,
    then returns the output
getstatusoutput(...): Runs a command in the shell, waits for it to complete,
    then returns a (exitcode, output) tuple
"""

import sys
from _typeshed import MaybeNone, ReadableBuffer, StrOrBytesPath
from collections.abc import Callable, Collection, Iterable, Mapping, Sequence
from types import GenericAlias, TracebackType
from typing import IO, Any, AnyStr, Final, Generic, Literal, TypeVar, overload
from typing_extensions import Self, TypeAlias

__all__ = [
    "Popen",
    "PIPE",
    "STDOUT",
    "call",
    "check_call",
    "getstatusoutput",
    "getoutput",
    "check_output",
    "run",
    "CalledProcessError",
    "DEVNULL",
    "SubprocessError",
    "TimeoutExpired",
    "CompletedProcess",
]

if sys.platform == "win32":
    __all__ += [
        "CREATE_NEW_CONSOLE",
        "CREATE_NEW_PROCESS_GROUP",
        "STARTF_USESHOWWINDOW",
        "STARTF_USESTDHANDLES",
        "STARTUPINFO",
        "STD_ERROR_HANDLE",
        "STD_INPUT_HANDLE",
        "STD_OUTPUT_HANDLE",
        "SW_HIDE",
        "ABOVE_NORMAL_PRIORITY_CLASS",
        "BELOW_NORMAL_PRIORITY_CLASS",
        "CREATE_BREAKAWAY_FROM_JOB",
        "CREATE_DEFAULT_ERROR_MODE",
        "CREATE_NO_WINDOW",
        "DETACHED_PROCESS",
        "HIGH_PRIORITY_CLASS",
        "IDLE_PRIORITY_CLASS",
        "NORMAL_PRIORITY_CLASS",
        "REALTIME_PRIORITY_CLASS",
    ]

# We prefer to annotate inputs to methods (eg subprocess.check_call) with these
# union types.
# For outputs we use laborious literal based overloads to try to determine
# which specific return types to use, and prefer to fall back to Any when
# this does not work, so the caller does not have to use an assertion to confirm
# which type.
#
# For example:
#
# try:
#    x = subprocess.check_output(["ls", "-l"])
#    reveal_type(x)  # bytes, based on the overloads
# except TimeoutError as e:
#    reveal_type(e.cmd)  # Any, but morally is _CMD
_FILE: TypeAlias = None | int | IO[Any]
_InputString: TypeAlias = ReadableBuffer | str
_CMD: TypeAlias = StrOrBytesPath | Sequence[StrOrBytesPath]
if sys.platform == "win32":
    _ENV: TypeAlias = Mapping[str, str]
else:
    _ENV: TypeAlias = Mapping[bytes, StrOrBytesPath] | Mapping[str, StrOrBytesPath]

_T = TypeVar("_T")

# These two are private but documented
if sys.version_info >= (3, 11):
    _USE_VFORK: Final[bool]
_USE_POSIX_SPAWN: Final[bool]

class CompletedProcess(Generic[_T]):
    """A process that has finished running.

    This is returned by run().

    Attributes:
      args: The list or str args passed to run().
      returncode: The exit code of the process, negative for signals.
      stdout: The standard output (None if not captured).
      stderr: The standard error (None if not captured).
    """

    # morally: _CMD
    args: Any
    returncode: int
    # These can both be None, but requiring checks for None would be tedious
    # and writing all the overloads would be horrific.
    stdout: _T
    stderr: _T
    def __init__(self, args: _CMD, returncode: int, stdout: _T | None = None, stderr: _T | None = None) -> None: ...
    def check_returncode(self) -> None:
        """Raise CalledProcessError if the exit code is non-zero."""

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

if sys.version_info >= (3, 11):
    # 3.11 adds "process_group" argument
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str | None = None,
        input: str | None = None,
        text: Literal[True],
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> CompletedProcess[str]:
        """Run command with arguments and return a CompletedProcess instance.

        The returned instance will have attributes args, returncode, stdout and
        stderr. By default, stdout and stderr are not captured, and those attributes
        will be None. Pass stdout=PIPE and/or stderr=PIPE in order to capture them,
        or pass capture_output=True to capture both.

        If check is True and the exit code was non-zero, it raises a
        CalledProcessError. The CalledProcessError object will have the return code
        in the returncode attribute, and output & stderr attributes if those streams
        were captured.

        If timeout (seconds) is given and the process takes too long,
         a TimeoutExpired exception will be raised.

        There is an optional argument "input", allowing you to
        pass bytes or a string to the subprocess's stdin.  If you use this argument
        you may not also use the Popen constructor's "stdin" argument, as
        it will be used internally.

        By default, all communication is in bytes, and therefore any "input" should
        be bytes, and the stdout and stderr will be bytes. If in text mode, any
        "input" should be a string, and stdout and stderr will be strings decoded
        according to locale encoding, or by "encoding" if set. Text mode is
        triggered by setting any of text, encoding, errors or universal_newlines.

        The other arguments are the same as for the Popen constructor.
        """

    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str,
        errors: str | None = None,
        input: str | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> CompletedProcess[str]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str,
        input: str | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> CompletedProcess[str]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        *,
        universal_newlines: Literal[True],
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        # where the *real* keyword only args start
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str | None = None,
        input: str | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> CompletedProcess[str]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: Literal[False] | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: None = None,
        errors: None = None,
        input: ReadableBuffer | None = None,
        text: Literal[False] | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> CompletedProcess[bytes]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str | None = None,
        input: _InputString | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> CompletedProcess[Any]: ...

elif sys.version_info >= (3, 10):
    # 3.10 adds "pipesize" argument
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str | None = None,
        input: str | None = None,
        text: Literal[True],
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> CompletedProcess[str]:
        """Run command with arguments and return a CompletedProcess instance.

        The returned instance will have attributes args, returncode, stdout and
        stderr. By default, stdout and stderr are not captured, and those attributes
        will be None. Pass stdout=PIPE and/or stderr=PIPE in order to capture them,
        or pass capture_output=True to capture both.

        If check is True and the exit code was non-zero, it raises a
        CalledProcessError. The CalledProcessError object will have the return code
        in the returncode attribute, and output & stderr attributes if those streams
        were captured.

        If timeout is given, and the process takes too long, a TimeoutExpired
        exception will be raised.

        There is an optional argument "input", allowing you to
        pass bytes or a string to the subprocess's stdin.  If you use this argument
        you may not also use the Popen constructor's "stdin" argument, as
        it will be used internally.

        By default, all communication is in bytes, and therefore any "input" should
        be bytes, and the stdout and stderr will be bytes. If in text mode, any
        "input" should be a string, and stdout and stderr will be strings decoded
        according to locale encoding, or by "encoding" if set. Text mode is
        triggered by setting any of text, encoding, errors or universal_newlines.

        The other arguments are the same as for the Popen constructor.
        """

    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str,
        errors: str | None = None,
        input: str | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> CompletedProcess[str]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str,
        input: str | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> CompletedProcess[str]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        *,
        universal_newlines: Literal[True],
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        # where the *real* keyword only args start
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str | None = None,
        input: str | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> CompletedProcess[str]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: Literal[False] | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: None = None,
        errors: None = None,
        input: ReadableBuffer | None = None,
        text: Literal[False] | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> CompletedProcess[bytes]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str | None = None,
        input: _InputString | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> CompletedProcess[Any]: ...

else:
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str | None = None,
        input: str | None = None,
        text: Literal[True],
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> CompletedProcess[str]:
        """Run command with arguments and return a CompletedProcess instance.

        The returned instance will have attributes args, returncode, stdout and
        stderr. By default, stdout and stderr are not captured, and those attributes
        will be None. Pass stdout=PIPE and/or stderr=PIPE in order to capture them.

        If check is True and the exit code was non-zero, it raises a
        CalledProcessError. The CalledProcessError object will have the return code
        in the returncode attribute, and output & stderr attributes if those streams
        were captured.

        If timeout is given, and the process takes too long, a TimeoutExpired
        exception will be raised.

        There is an optional argument "input", allowing you to
        pass bytes or a string to the subprocess's stdin.  If you use this argument
        you may not also use the Popen constructor's "stdin" argument, as
        it will be used internally.

        By default, all communication is in bytes, and therefore any "input" should
        be bytes, and the stdout and stderr will be bytes. If in text mode, any
        "input" should be a string, and stdout and stderr will be strings decoded
        according to locale encoding, or by "encoding" if set. Text mode is
        triggered by setting any of text, encoding, errors or universal_newlines.

        The other arguments are the same as for the Popen constructor.
        """

    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str,
        errors: str | None = None,
        input: str | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> CompletedProcess[str]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str,
        input: str | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> CompletedProcess[str]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        *,
        universal_newlines: Literal[True],
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        # where the *real* keyword only args start
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str | None = None,
        input: str | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> CompletedProcess[str]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: Literal[False] | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: None = None,
        errors: None = None,
        input: ReadableBuffer | None = None,
        text: Literal[False] | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> CompletedProcess[bytes]: ...
    @overload
    def run(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        capture_output: bool = False,
        check: bool = False,
        encoding: str | None = None,
        errors: str | None = None,
        input: _InputString | None = None,
        text: bool | None = None,
        timeout: float | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> CompletedProcess[Any]: ...

# Same args as Popen.__init__
if sys.version_info >= (3, 11):
    # 3.11 adds "process_group" argument
    def call(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        encoding: str | None = None,
        timeout: float | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> int:
        """Run command with arguments.  Wait for command to complete or
        for timeout seconds, then return the returncode attribute.

        The arguments are the same as for the Popen constructor.  Example:

        retcode = call(["ls", "-l"])
        """

elif sys.version_info >= (3, 10):
    # 3.10 adds "pipesize" argument
    def call(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        encoding: str | None = None,
        timeout: float | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> int:
        """Run command with arguments.  Wait for command to complete or
        timeout, then return the returncode attribute.

        The arguments are the same as for the Popen constructor.  Example:

        retcode = call(["ls", "-l"])
        """

else:
    def call(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        encoding: str | None = None,
        timeout: float | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> int:
        """Run command with arguments.  Wait for command to complete or
        timeout, then return the returncode attribute.

        The arguments are the same as for the Popen constructor.  Example:

        retcode = call(["ls", "-l"])
        """

# Same args as Popen.__init__
if sys.version_info >= (3, 11):
    # 3.11 adds "process_group" argument
    def check_call(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        timeout: float | None = None,
        *,
        encoding: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> int:
        """Run command with arguments.  Wait for command to complete.  If
        the exit code was zero then return, otherwise raise
        CalledProcessError.  The CalledProcessError object will have the
        return code in the returncode attribute.

        The arguments are the same as for the call function.  Example:

        check_call(["ls", "-l"])
        """

elif sys.version_info >= (3, 10):
    # 3.10 adds "pipesize" argument
    def check_call(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        timeout: float | None = None,
        *,
        encoding: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> int:
        """Run command with arguments.  Wait for command to complete.  If
        the exit code was zero then return, otherwise raise
        CalledProcessError.  The CalledProcessError object will have the
        return code in the returncode attribute.

        The arguments are the same as for the call function.  Example:

        check_call(["ls", "-l"])
        """

else:
    def check_call(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stdout: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        timeout: float | None = None,
        *,
        encoding: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> int:
        """Run command with arguments.  Wait for command to complete.  If
        the exit code was zero then return, otherwise raise
        CalledProcessError.  The CalledProcessError object will have the
        return code in the returncode attribute.

        The arguments are the same as for the call function.  Example:

        check_call(["ls", "-l"])
        """

if sys.version_info >= (3, 11):
    # 3.11 adds "process_group" argument
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str | None = None,
        text: Literal[True],
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> str:
        """Run command with arguments and return its output.

        If the exit code was non-zero it raises a CalledProcessError.  The
        CalledProcessError object will have the return code in the returncode
        attribute and output in the output attribute.

        The arguments are the same as for the Popen constructor.  Example:

        >>> check_output(["ls", "-l", "/dev/null"])
        b'crw-rw-rw- 1 root root 1, 3 Oct 18  2007 /dev/null\\n'

        The stdout argument is not allowed as it is used internally.
        To capture standard error in the result, use stderr=STDOUT.

        >>> check_output(["/bin/sh", "-c",
        ...               "ls -l non_existent_file ; exit 0"],
        ...              stderr=STDOUT)
        b'ls: non_existent_file: No such file or directory\\n'

        There is an additional optional argument, "input", allowing you to
        pass a string to the subprocess's stdin.  If you use this argument
        you may not also use the Popen constructor's "stdin" argument, as
        it too will be used internally.  Example:

        >>> check_output(["sed", "-e", "s/foo/bar/"],
        ...              input=b"when in the course of fooman events\\n")
        b'when in the course of barman events\\n'

        By default, all communication is in bytes, and therefore any "input"
        should be bytes, and the return value will be bytes.  If in text mode,
        any "input" should be a string, and the return value will be a string
        decoded according to locale encoding, or by "encoding" if set. Text mode
        is triggered by setting any of text, encoding, errors or universal_newlines.
        """

    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str,
        errors: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> str: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> str: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        *,
        universal_newlines: Literal[True],
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        # where the real keyword only ones start
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> str: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: Literal[False] | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: None = None,
        errors: None = None,
        text: Literal[False] | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> bytes: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
        process_group: int | None = None,
    ) -> Any: ...  # morally: -> str | bytes

elif sys.version_info >= (3, 10):
    # 3.10 adds "pipesize" argument
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str | None = None,
        text: Literal[True],
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> str:
        """Run command with arguments and return its output.

        If the exit code was non-zero it raises a CalledProcessError.  The
        CalledProcessError object will have the return code in the returncode
        attribute and output in the output attribute.

        The arguments are the same as for the Popen constructor.  Example:

        >>> check_output(["ls", "-l", "/dev/null"])
        b'crw-rw-rw- 1 root root 1, 3 Oct 18  2007 /dev/null\\n'

        The stdout argument is not allowed as it is used internally.
        To capture standard error in the result, use stderr=STDOUT.

        >>> check_output(["/bin/sh", "-c",
        ...               "ls -l non_existent_file ; exit 0"],
        ...              stderr=STDOUT)
        b'ls: non_existent_file: No such file or directory\\n'

        There is an additional optional argument, "input", allowing you to
        pass a string to the subprocess's stdin.  If you use this argument
        you may not also use the Popen constructor's "stdin" argument, as
        it too will be used internally.  Example:

        >>> check_output(["sed", "-e", "s/foo/bar/"],
        ...              input=b"when in the course of fooman events\\n")
        b'when in the course of barman events\\n'

        By default, all communication is in bytes, and therefore any "input"
        should be bytes, and the return value will be bytes.  If in text mode,
        any "input" should be a string, and the return value will be a string
        decoded according to locale encoding, or by "encoding" if set. Text mode
        is triggered by setting any of text, encoding, errors or universal_newlines.
        """

    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str,
        errors: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> str: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> str: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        *,
        universal_newlines: Literal[True],
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        # where the real keyword only ones start
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> str: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: Literal[False] | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: None = None,
        errors: None = None,
        text: Literal[False] | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> bytes: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
        pipesize: int = -1,
    ) -> Any: ...  # morally: -> str | bytes

else:
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str | None = None,
        text: Literal[True],
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> str:
        """Run command with arguments and return its output.

        If the exit code was non-zero it raises a CalledProcessError.  The
        CalledProcessError object will have the return code in the returncode
        attribute and output in the output attribute.

        The arguments are the same as for the Popen constructor.  Example:

        >>> check_output(["ls", "-l", "/dev/null"])
        b'crw-rw-rw- 1 root root 1, 3 Oct 18  2007 /dev/null\\n'

        The stdout argument is not allowed as it is used internally.
        To capture standard error in the result, use stderr=STDOUT.

        >>> check_output(["/bin/sh", "-c",
        ...               "ls -l non_existent_file ; exit 0"],
        ...              stderr=STDOUT)
        b'ls: non_existent_file: No such file or directory\\n'

        There is an additional optional argument, "input", allowing you to
        pass a string to the subprocess's stdin.  If you use this argument
        you may not also use the Popen constructor's "stdin" argument, as
        it too will be used internally.  Example:

        >>> check_output(["sed", "-e", "s/foo/bar/"],
        ...              input=b"when in the course of fooman events\\n")
        b'when in the course of barman events\\n'

        By default, all communication is in bytes, and therefore any "input"
        should be bytes, and the return value will be bytes.  If in text mode,
        any "input" should be a string, and the return value will be a string
        decoded according to locale encoding, or by "encoding" if set. Text mode
        is triggered by setting any of text, encoding, errors or universal_newlines.
        """

    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str,
        errors: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> str: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> str: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        *,
        universal_newlines: Literal[True],
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        # where the real keyword only ones start
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> str: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: Literal[False] | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: None = None,
        errors: None = None,
        text: Literal[False] | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> bytes: ...
    @overload
    def check_output(
        args: _CMD,
        bufsize: int = -1,
        executable: StrOrBytesPath | None = None,
        stdin: _FILE = None,
        stderr: _FILE = None,
        preexec_fn: Callable[[], Any] | None = None,
        close_fds: bool = True,
        shell: bool = False,
        cwd: StrOrBytesPath | None = None,
        env: _ENV | None = None,
        universal_newlines: bool | None = None,
        startupinfo: Any = None,
        creationflags: int = 0,
        restore_signals: bool = True,
        start_new_session: bool = False,
        pass_fds: Collection[int] = (),
        *,
        timeout: float | None = None,
        input: _InputString | None = None,
        encoding: str | None = None,
        errors: str | None = None,
        text: bool | None = None,
        user: str | int | None = None,
        group: str | int | None = None,
        extra_groups: Iterable[str | int] | None = None,
        umask: int = -1,
    ) -> Any: ...  # morally: -> str | bytes

PIPE: Final[int]
STDOUT: Final[int]
DEVNULL: Final[int]

class SubprocessError(Exception): ...

class TimeoutExpired(SubprocessError):
    """This exception is raised when the timeout expires while waiting for a
    child process.

    Attributes:
        cmd, output, stdout, stderr, timeout
    """

    def __init__(
        self, cmd: _CMD, timeout: float, output: str | bytes | None = None, stderr: str | bytes | None = None
    ) -> None: ...
    # morally: _CMD
    cmd: Any
    timeout: float
    # morally: str | bytes | None
    output: Any
    stdout: bytes | None
    stderr: bytes | None

class CalledProcessError(SubprocessError):
    """Raised when run() is called with check=True and the process
    returns a non-zero exit status.

    Attributes:
      cmd, returncode, stdout, stderr, output
    """

    returncode: int
    # morally: _CMD
    cmd: Any
    # morally: str | bytes | None
    output: Any

    # morally: str | bytes | None
    stdout: Any
    stderr: Any
    def __init__(
        self, returncode: int, cmd: _CMD, output: str | bytes | None = None, stderr: str | bytes | None = None
    ) -> None: ...

class Popen(Generic[AnyStr]):
    """Execute a child program in a new process.

    For a complete description of the arguments see the Python documentation.

    Arguments:
      args: A string, or a sequence of program arguments.

      bufsize: supplied as the buffering argument to the open() function when
          creating the stdin/stdout/stderr pipe file objects

      executable: A replacement program to execute.

      stdin, stdout and stderr: These specify the executed programs' standard
          input, standard output and standard error file handles, respectively.

      preexec_fn: (POSIX only) An object to be called in the child process
          just before the child is executed.

      close_fds: Controls closing or inheriting of file descriptors.

      shell: If true, the command will be executed through the shell.

      cwd: Sets the current directory before the child is executed.

      env: Defines the environment variables for the new process.

      text: If true, decode stdin, stdout and stderr using the given encoding
          (if set) or the system default otherwise.

      universal_newlines: Alias of text, provided for backwards compatibility.

      startupinfo and creationflags (Windows only)

      restore_signals (POSIX only)

      start_new_session (POSIX only)

      process_group (POSIX only)

      group (POSIX only)

      extra_groups (POSIX only)

      user (POSIX only)

      umask (POSIX only)

      pass_fds (POSIX only)

      encoding and errors: Text mode encoding and error handling to use for
          file objects stdin, stdout and stderr.

    Attributes:
        stdin, stdout, stderr, pid, returncode
    """

    args: _CMD
    stdin: IO[AnyStr] | None
    stdout: IO[AnyStr] | None
    stderr: IO[AnyStr] | None
    pid: int
    returncode: int | MaybeNone
    universal_newlines: bool

    if sys.version_info >= (3, 11):
        # process_group is added in 3.11
        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: bool | None = None,
            encoding: str,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
            process_group: int | None = None,
        ) -> None:
            """Create new Popen instance."""

        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: bool | None = None,
            encoding: str | None = None,
            errors: str,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
            process_group: int | None = None,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            *,
            universal_newlines: Literal[True],
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            # where the *real* keyword only args start
            text: bool | None = None,
            encoding: str | None = None,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
            process_group: int | None = None,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: Literal[True],
            encoding: str | None = None,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
            process_group: int | None = None,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[bytes],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: Literal[False] | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: Literal[False] | None = None,
            encoding: None = None,
            errors: None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
            process_group: int | None = None,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[Any],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: bool | None = None,
            encoding: str | None = None,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
            process_group: int | None = None,
        ) -> None: ...
    elif sys.version_info >= (3, 10):
        # pipesize is added in 3.10
        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: bool | None = None,
            encoding: str,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
        ) -> None:
            """Create new Popen instance."""

        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: bool | None = None,
            encoding: str | None = None,
            errors: str,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            *,
            universal_newlines: Literal[True],
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            # where the *real* keyword only args start
            text: bool | None = None,
            encoding: str | None = None,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: Literal[True],
            encoding: str | None = None,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[bytes],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: Literal[False] | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: Literal[False] | None = None,
            encoding: None = None,
            errors: None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[Any],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: bool | None = None,
            encoding: str | None = None,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
            pipesize: int = -1,
        ) -> None: ...
    else:
        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: bool | None = None,
            encoding: str,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
        ) -> None:
            """Create new Popen instance."""

        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: bool | None = None,
            encoding: str | None = None,
            errors: str,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            *,
            universal_newlines: Literal[True],
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            # where the *real* keyword only args start
            text: bool | None = None,
            encoding: str | None = None,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[str],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: Literal[True],
            encoding: str | None = None,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[bytes],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: Literal[False] | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: Literal[False] | None = None,
            encoding: None = None,
            errors: None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
        ) -> None: ...
        @overload
        def __init__(
            self: Popen[Any],
            args: _CMD,
            bufsize: int = -1,
            executable: StrOrBytesPath | None = None,
            stdin: _FILE | None = None,
            stdout: _FILE | None = None,
            stderr: _FILE | None = None,
            preexec_fn: Callable[[], Any] | None = None,
            close_fds: bool = True,
            shell: bool = False,
            cwd: StrOrBytesPath | None = None,
            env: _ENV | None = None,
            universal_newlines: bool | None = None,
            startupinfo: Any | None = None,
            creationflags: int = 0,
            restore_signals: bool = True,
            start_new_session: bool = False,
            pass_fds: Collection[int] = (),
            *,
            text: bool | None = None,
            encoding: str | None = None,
            errors: str | None = None,
            user: str | int | None = None,
            group: str | int | None = None,
            extra_groups: Iterable[str | int] | None = None,
            umask: int = -1,
        ) -> None: ...

    def poll(self) -> int | None:
        """Check if child process has terminated. Set and return returncode
        attribute.
        """

    def wait(self, timeout: float | None = None) -> int:
        """Wait for child process to terminate; returns self.returncode."""
    # morally the members of the returned tuple should be optional
    # TODO: this should allow ReadableBuffer for Popen[bytes], but adding
    # overloads for that runs into a mypy bug (python/mypy#14070).
    def communicate(self, input: AnyStr | None = None, timeout: float | None = None) -> tuple[AnyStr, AnyStr]:
        """Interact with process: Send data to stdin and close it.
        Read data from stdout and stderr, until end-of-file is
        reached.  Wait for process to terminate.

        The optional "input" argument should be data to be sent to the
        child process, or None, if no data should be sent to the child.
        communicate() returns a tuple (stdout, stderr).

        By default, all communication is in bytes, and therefore any
        "input" should be bytes, and the (stdout, stderr) will be bytes.
        If in text mode (indicated by self.text_mode), any "input" should
        be a string, and (stdout, stderr) will be strings decoded
        according to locale encoding, or by "encoding" if set. Text mode
        is triggered by setting any of text, encoding, errors or
        universal_newlines.
        """

    def send_signal(self, sig: int) -> None:
        """Send a signal to the process."""

    def terminate(self) -> None:
        """Terminate the process with SIGTERM"""

    def kill(self) -> None:
        """Kill the process with SIGKILL"""

    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
    ) -> None: ...
    def __del__(self) -> None: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

# The result really is always a str.
if sys.version_info >= (3, 11):
    def getstatusoutput(cmd: _CMD, *, encoding: str | None = None, errors: str | None = None) -> tuple[int, str]:
        """Return (exitcode, output) of executing cmd in a shell.

        Execute the string 'cmd' in a shell with 'check_output' and
        return a 2-tuple (status, output). The locale encoding is used
        to decode the output and process newlines.

        A trailing newline is stripped from the output.
        The exit status for the command can be interpreted
        according to the rules for the function 'wait'. Example:

        >>> import subprocess
        >>> subprocess.getstatusoutput('ls /bin/ls')
        (0, '/bin/ls')
        >>> subprocess.getstatusoutput('cat /bin/junk')
        (1, 'cat: /bin/junk: No such file or directory')
        >>> subprocess.getstatusoutput('/bin/junk')
        (127, 'sh: /bin/junk: not found')
        >>> subprocess.getstatusoutput('/bin/kill $$')
        (-15, '')
        """

    def getoutput(cmd: _CMD, *, encoding: str | None = None, errors: str | None = None) -> str:
        """Return output (stdout or stderr) of executing cmd in a shell.

        Like getstatusoutput(), except the exit status is ignored and the return
        value is a string containing the command's output.  Example:

        >>> import subprocess
        >>> subprocess.getoutput('ls /bin/ls')
        '/bin/ls'
        """

else:
    def getstatusoutput(cmd: _CMD) -> tuple[int, str]:
        """Return (exitcode, output) of executing cmd in a shell.

        Execute the string 'cmd' in a shell with 'check_output' and
        return a 2-tuple (status, output). The locale encoding is used
        to decode the output and process newlines.

        A trailing newline is stripped from the output.
        The exit status for the command can be interpreted
        according to the rules for the function 'wait'. Example:

        >>> import subprocess
        >>> subprocess.getstatusoutput('ls /bin/ls')
        (0, '/bin/ls')
        >>> subprocess.getstatusoutput('cat /bin/junk')
        (1, 'cat: /bin/junk: No such file or directory')
        >>> subprocess.getstatusoutput('/bin/junk')
        (127, 'sh: /bin/junk: not found')
        >>> subprocess.getstatusoutput('/bin/kill $$')
        (-15, '')
        """

    def getoutput(cmd: _CMD) -> str:
        """Return output (stdout or stderr) of executing cmd in a shell.

        Like getstatusoutput(), except the exit status is ignored and the return
        value is a string containing the command's output.  Example:

        >>> import subprocess
        >>> subprocess.getoutput('ls /bin/ls')
        '/bin/ls'
        """

def list2cmdline(seq: Iterable[StrOrBytesPath]) -> str:  # undocumented
    """
    Translate a sequence of arguments into a command line
    string, using the same rules as the MS C runtime:

    1) Arguments are delimited by white space, which is either a
       space or a tab.

    2) A string surrounded by double quotation marks is
       interpreted as a single argument, regardless of white space
       contained within.  A quoted string can be embedded in an
       argument.

    3) A double quotation mark preceded by a backslash is
       interpreted as a literal double quotation mark.

    4) Backslashes are interpreted literally, unless they
       immediately precede a double quotation mark.

    5) If backslashes immediately precede a double quotation mark,
       every pair of backslashes is interpreted as a literal
       backslash.  If the number of backslashes is odd, the last
       backslash escapes the next double quotation mark as
       described in rule 3.
    """

if sys.platform == "win32":
    if sys.version_info >= (3, 13):
        from _winapi import STARTF_FORCEOFFFEEDBACK, STARTF_FORCEONFEEDBACK

        __all__ += ["STARTF_FORCEOFFFEEDBACK", "STARTF_FORCEONFEEDBACK"]

    class STARTUPINFO:
        def __init__(
            self,
            *,
            dwFlags: int = 0,
            hStdInput: Any | None = None,
            hStdOutput: Any | None = None,
            hStdError: Any | None = None,
            wShowWindow: int = 0,
            lpAttributeList: Mapping[str, Any] | None = None,
        ) -> None: ...
        dwFlags: int
        hStdInput: Any | None
        hStdOutput: Any | None
        hStdError: Any | None
        wShowWindow: int
        lpAttributeList: Mapping[str, Any]
        def copy(self) -> STARTUPINFO: ...

    from _winapi import (
        ABOVE_NORMAL_PRIORITY_CLASS as ABOVE_NORMAL_PRIORITY_CLASS,
        BELOW_NORMAL_PRIORITY_CLASS as BELOW_NORMAL_PRIORITY_CLASS,
        CREATE_BREAKAWAY_FROM_JOB as CREATE_BREAKAWAY_FROM_JOB,
        CREATE_DEFAULT_ERROR_MODE as CREATE_DEFAULT_ERROR_MODE,
        CREATE_NEW_CONSOLE as CREATE_NEW_CONSOLE,
        CREATE_NEW_PROCESS_GROUP as CREATE_NEW_PROCESS_GROUP,
        CREATE_NO_WINDOW as CREATE_NO_WINDOW,
        DETACHED_PROCESS as DETACHED_PROCESS,
        HIGH_PRIORITY_CLASS as HIGH_PRIORITY_CLASS,
        IDLE_PRIORITY_CLASS as IDLE_PRIORITY_CLASS,
        NORMAL_PRIORITY_CLASS as NORMAL_PRIORITY_CLASS,
        REALTIME_PRIORITY_CLASS as REALTIME_PRIORITY_CLASS,
        STARTF_USESHOWWINDOW as STARTF_USESHOWWINDOW,
        STARTF_USESTDHANDLES as STARTF_USESTDHANDLES,
        STD_ERROR_HANDLE as STD_ERROR_HANDLE,
        STD_INPUT_HANDLE as STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE as STD_OUTPUT_HANDLE,
        SW_HIDE as SW_HIDE,
    )
