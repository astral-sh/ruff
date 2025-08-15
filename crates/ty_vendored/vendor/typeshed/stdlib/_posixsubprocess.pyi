"""A POSIX helper for the subprocess module."""

import sys
from _typeshed import StrOrBytesPath
from collections.abc import Callable, Sequence
from typing import SupportsIndex

if sys.platform != "win32":
    if sys.version_info >= (3, 14):
        def fork_exec(
            args: Sequence[StrOrBytesPath] | None,
            executable_list: Sequence[bytes],
            close_fds: bool,
            pass_fds: tuple[int, ...],
            cwd: str,
            env: Sequence[bytes] | None,
            p2cread: int,
            p2cwrite: int,
            c2pread: int,
            c2pwrite: int,
            errread: int,
            errwrite: int,
            errpipe_read: int,
            errpipe_write: int,
            restore_signals: int,
            call_setsid: int,
            pgid_to_set: int,
            gid: SupportsIndex | None,
            extra_groups: list[int] | None,
            uid: SupportsIndex | None,
            child_umask: int,
            preexec_fn: Callable[[], None],
            /,
        ) -> int:
            """Spawn a fresh new child process.

            Fork a child process, close parent file descriptors as appropriate in the
            child and duplicate the few that are needed before calling exec() in the
            child process.

            If close_fds is True, close file descriptors 3 and higher, except those listed
            in the sorted tuple pass_fds.

            The preexec_fn, if supplied, will be called immediately before closing file
            descriptors and exec.

            WARNING: preexec_fn is NOT SAFE if your application uses threads.
                     It may trigger infrequent, difficult to debug deadlocks.

            If an error occurs in the child process before the exec, it is
            serialized and written to the errpipe_write fd per subprocess.py.

            Returns: the child process's PID.

            Raises: Only on an error in the parent process.
            """
    else:
        def fork_exec(
            args: Sequence[StrOrBytesPath] | None,
            executable_list: Sequence[bytes],
            close_fds: bool,
            pass_fds: tuple[int, ...],
            cwd: str,
            env: Sequence[bytes] | None,
            p2cread: int,
            p2cwrite: int,
            c2pread: int,
            c2pwrite: int,
            errread: int,
            errwrite: int,
            errpipe_read: int,
            errpipe_write: int,
            restore_signals: bool,
            call_setsid: bool,
            pgid_to_set: int,
            gid: SupportsIndex | None,
            extra_groups: list[int] | None,
            uid: SupportsIndex | None,
            child_umask: int,
            preexec_fn: Callable[[], None],
            allow_vfork: bool,
            /,
        ) -> int:
            """Spawn a fresh new child process.

            Fork a child process, close parent file descriptors as appropriate in the
            child and duplicate the few that are needed before calling exec() in the
            child process.

            If close_fds is True, close file descriptors 3 and higher, except those listed
            in the sorted tuple pass_fds.

            The preexec_fn, if supplied, will be called immediately before closing file
            descriptors and exec.

            WARNING: preexec_fn is NOT SAFE if your application uses threads.
                     It may trigger infrequent, difficult to debug deadlocks.

            If an error occurs in the child process before the exec, it is
            serialized and written to the errpipe_write fd per subprocess.py.

            Returns: the child process's PID.

            Raises: Only on an error in the parent process.
            """
