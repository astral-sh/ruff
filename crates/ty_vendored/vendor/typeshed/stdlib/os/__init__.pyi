"""OS routines for NT or Posix depending on what system we're on.

This exports:
  - all functions from posix or nt, e.g. unlink, stat, etc.
  - os.path is either posixpath or ntpath
  - os.name is either 'posix' or 'nt'
  - os.curdir is a string representing the current directory (always '.')
  - os.pardir is a string representing the parent directory (always '..')
  - os.sep is the (or a most common) pathname separator ('/' or '\\\\')
  - os.extsep is the extension separator (always '.')
  - os.altsep is the alternate pathname separator (None or '/')
  - os.pathsep is the component separator used in $PATH etc
  - os.linesep is the line separator in text files ('\\n' or '\\r\\n')
  - os.defpath is the default search path for executables
  - os.devnull is the file path of the null device ('/dev/null', etc.)

Programs that import and use 'os' stand a better chance of being
portable between different platforms.  Of course, they must then
only use functions that are defined by all platforms (e.g., unlink
and opendir), and leave all pathname manipulation to os.path
(e.g., split and join).
"""

import sys
from _typeshed import (
    AnyStr_co,
    BytesPath,
    FileDescriptor,
    FileDescriptorLike,
    FileDescriptorOrPath,
    GenericPath,
    OpenBinaryMode,
    OpenBinaryModeReading,
    OpenBinaryModeUpdating,
    OpenBinaryModeWriting,
    OpenTextMode,
    ReadableBuffer,
    StrOrBytesPath,
    StrPath,
    SupportsLenAndGetItem,
    Unused,
    WriteableBuffer,
    structseq,
)
from abc import ABC, abstractmethod
from builtins import OSError
from collections.abc import Callable, Iterable, Iterator, Mapping, MutableMapping, Sequence
from io import BufferedRandom, BufferedReader, BufferedWriter, FileIO, TextIOWrapper
from subprocess import Popen
from types import GenericAlias, TracebackType
from typing import (
    IO,
    Any,
    AnyStr,
    BinaryIO,
    Final,
    Generic,
    Literal,
    NoReturn,
    Protocol,
    TypeVar,
    final,
    overload,
    runtime_checkable,
    type_check_only,
)
from typing_extensions import LiteralString, Self, TypeAlias, Unpack, deprecated

from . import path as _path

# Re-export common definitions from os.path to reduce duplication
from .path import (
    altsep as altsep,
    curdir as curdir,
    defpath as defpath,
    devnull as devnull,
    extsep as extsep,
    pardir as pardir,
    pathsep as pathsep,
    sep as sep,
)

__all__ = [
    "F_OK",
    "O_APPEND",
    "O_CREAT",
    "O_EXCL",
    "O_RDONLY",
    "O_RDWR",
    "O_TRUNC",
    "O_WRONLY",
    "P_NOWAIT",
    "P_NOWAITO",
    "P_WAIT",
    "R_OK",
    "SEEK_CUR",
    "SEEK_END",
    "SEEK_SET",
    "TMP_MAX",
    "W_OK",
    "X_OK",
    "DirEntry",
    "_exit",
    "abort",
    "access",
    "altsep",
    "chdir",
    "chmod",
    "close",
    "closerange",
    "cpu_count",
    "curdir",
    "defpath",
    "device_encoding",
    "devnull",
    "dup",
    "dup2",
    "environ",
    "error",
    "execl",
    "execle",
    "execlp",
    "execlpe",
    "execv",
    "execve",
    "execvp",
    "execvpe",
    "extsep",
    "fdopen",
    "fsdecode",
    "fsencode",
    "fspath",
    "fstat",
    "fsync",
    "ftruncate",
    "get_exec_path",
    "get_inheritable",
    "get_terminal_size",
    "getcwd",
    "getcwdb",
    "getenv",
    "getlogin",
    "getpid",
    "getppid",
    "isatty",
    "kill",
    "linesep",
    "link",
    "listdir",
    "lseek",
    "lstat",
    "makedirs",
    "mkdir",
    "name",
    "open",
    "pardir",
    "path",
    "pathsep",
    "pipe",
    "popen",
    "putenv",
    "read",
    "readlink",
    "remove",
    "removedirs",
    "rename",
    "renames",
    "replace",
    "rmdir",
    "scandir",
    "sep",
    "set_inheritable",
    "spawnl",
    "spawnle",
    "spawnv",
    "spawnve",
    "stat",
    "stat_result",
    "statvfs_result",
    "strerror",
    "supports_bytes_environ",
    "symlink",
    "system",
    "terminal_size",
    "times",
    "times_result",
    "truncate",
    "umask",
    "uname_result",
    "unlink",
    "unsetenv",
    "urandom",
    "utime",
    "waitpid",
    "waitstatus_to_exitcode",
    "walk",
    "write",
]
if sys.version_info >= (3, 14):
    # reload_environ was added to __all__ in Python 3.14.1
    __all__ += ["readinto", "reload_environ"]
if sys.platform == "darwin" and sys.version_info >= (3, 12):
    __all__ += ["PRIO_DARWIN_BG", "PRIO_DARWIN_NONUI", "PRIO_DARWIN_PROCESS", "PRIO_DARWIN_THREAD"]
if sys.platform == "darwin" and sys.version_info >= (3, 10):
    __all__ += ["O_EVTONLY", "O_NOFOLLOW_ANY", "O_SYMLINK"]
if sys.platform == "linux":
    __all__ += [
        "GRND_NONBLOCK",
        "GRND_RANDOM",
        "MFD_ALLOW_SEALING",
        "MFD_CLOEXEC",
        "MFD_HUGETLB",
        "MFD_HUGE_16GB",
        "MFD_HUGE_16MB",
        "MFD_HUGE_1GB",
        "MFD_HUGE_1MB",
        "MFD_HUGE_256MB",
        "MFD_HUGE_2GB",
        "MFD_HUGE_2MB",
        "MFD_HUGE_32MB",
        "MFD_HUGE_512KB",
        "MFD_HUGE_512MB",
        "MFD_HUGE_64KB",
        "MFD_HUGE_8MB",
        "MFD_HUGE_MASK",
        "MFD_HUGE_SHIFT",
        "O_DIRECT",
        "O_LARGEFILE",
        "O_NOATIME",
        "O_PATH",
        "O_RSYNC",
        "O_TMPFILE",
        "P_PIDFD",
        "RTLD_DEEPBIND",
        "SCHED_BATCH",
        "SCHED_IDLE",
        "SCHED_RESET_ON_FORK",
        "XATTR_CREATE",
        "XATTR_REPLACE",
        "XATTR_SIZE_MAX",
        "copy_file_range",
        "getrandom",
        "getxattr",
        "listxattr",
        "memfd_create",
        "pidfd_open",
        "removexattr",
        "setxattr",
    ]
if sys.platform == "linux" and sys.version_info >= (3, 14):
    __all__ += ["SCHED_DEADLINE", "SCHED_NORMAL"]
if sys.platform == "linux" and sys.version_info >= (3, 13):
    __all__ += [
        "POSIX_SPAWN_CLOSEFROM",
        "TFD_CLOEXEC",
        "TFD_NONBLOCK",
        "TFD_TIMER_ABSTIME",
        "TFD_TIMER_CANCEL_ON_SET",
        "timerfd_create",
        "timerfd_gettime",
        "timerfd_gettime_ns",
        "timerfd_settime",
        "timerfd_settime_ns",
    ]
if sys.platform == "linux" and sys.version_info >= (3, 12):
    __all__ += [
        "CLONE_FILES",
        "CLONE_FS",
        "CLONE_NEWCGROUP",
        "CLONE_NEWIPC",
        "CLONE_NEWNET",
        "CLONE_NEWNS",
        "CLONE_NEWPID",
        "CLONE_NEWTIME",
        "CLONE_NEWUSER",
        "CLONE_NEWUTS",
        "CLONE_SIGHAND",
        "CLONE_SYSVSEM",
        "CLONE_THREAD",
        "CLONE_VM",
        "setns",
        "unshare",
        "PIDFD_NONBLOCK",
    ]
if sys.platform == "linux" and sys.version_info >= (3, 10):
    __all__ += [
        "EFD_CLOEXEC",
        "EFD_NONBLOCK",
        "EFD_SEMAPHORE",
        "RWF_APPEND",
        "SPLICE_F_MORE",
        "SPLICE_F_MOVE",
        "SPLICE_F_NONBLOCK",
        "eventfd",
        "eventfd_read",
        "eventfd_write",
        "splice",
    ]
if sys.platform == "win32":
    __all__ += [
        "O_BINARY",
        "O_NOINHERIT",
        "O_RANDOM",
        "O_SEQUENTIAL",
        "O_SHORT_LIVED",
        "O_TEMPORARY",
        "O_TEXT",
        "P_DETACH",
        "P_OVERLAY",
        "get_handle_inheritable",
        "set_handle_inheritable",
        "startfile",
    ]
if sys.platform == "win32" and sys.version_info >= (3, 12):
    __all__ += ["listdrives", "listmounts", "listvolumes"]
if sys.platform != "win32":
    __all__ += [
        "CLD_CONTINUED",
        "CLD_DUMPED",
        "CLD_EXITED",
        "CLD_KILLED",
        "CLD_STOPPED",
        "CLD_TRAPPED",
        "EX_CANTCREAT",
        "EX_CONFIG",
        "EX_DATAERR",
        "EX_IOERR",
        "EX_NOHOST",
        "EX_NOINPUT",
        "EX_NOPERM",
        "EX_NOUSER",
        "EX_OSERR",
        "EX_OSFILE",
        "EX_PROTOCOL",
        "EX_SOFTWARE",
        "EX_TEMPFAIL",
        "EX_UNAVAILABLE",
        "EX_USAGE",
        "F_LOCK",
        "F_TEST",
        "F_TLOCK",
        "F_ULOCK",
        "NGROUPS_MAX",
        "O_ACCMODE",
        "O_ASYNC",
        "O_CLOEXEC",
        "O_DIRECTORY",
        "O_DSYNC",
        "O_NDELAY",
        "O_NOCTTY",
        "O_NOFOLLOW",
        "O_NONBLOCK",
        "O_SYNC",
        "POSIX_SPAWN_CLOSE",
        "POSIX_SPAWN_DUP2",
        "POSIX_SPAWN_OPEN",
        "PRIO_PGRP",
        "PRIO_PROCESS",
        "PRIO_USER",
        "P_ALL",
        "P_PGID",
        "P_PID",
        "RTLD_GLOBAL",
        "RTLD_LAZY",
        "RTLD_LOCAL",
        "RTLD_NODELETE",
        "RTLD_NOLOAD",
        "RTLD_NOW",
        "SCHED_FIFO",
        "SCHED_OTHER",
        "SCHED_RR",
        "SEEK_DATA",
        "SEEK_HOLE",
        "ST_NOSUID",
        "ST_RDONLY",
        "WCONTINUED",
        "WCOREDUMP",
        "WEXITED",
        "WEXITSTATUS",
        "WIFCONTINUED",
        "WIFEXITED",
        "WIFSIGNALED",
        "WIFSTOPPED",
        "WNOHANG",
        "WNOWAIT",
        "WSTOPPED",
        "WSTOPSIG",
        "WTERMSIG",
        "WUNTRACED",
        "chown",
        "chroot",
        "confstr",
        "confstr_names",
        "ctermid",
        "environb",
        "fchdir",
        "fchown",
        "fork",
        "forkpty",
        "fpathconf",
        "fstatvfs",
        "fwalk",
        "getegid",
        "getenvb",
        "geteuid",
        "getgid",
        "getgrouplist",
        "getgroups",
        "getloadavg",
        "getpgid",
        "getpgrp",
        "getpriority",
        "getsid",
        "getuid",
        "initgroups",
        "killpg",
        "lchown",
        "lockf",
        "major",
        "makedev",
        "minor",
        "mkfifo",
        "mknod",
        "nice",
        "openpty",
        "pathconf",
        "pathconf_names",
        "posix_spawn",
        "posix_spawnp",
        "pread",
        "preadv",
        "pwrite",
        "pwritev",
        "readv",
        "register_at_fork",
        "sched_get_priority_max",
        "sched_get_priority_min",
        "sched_yield",
        "sendfile",
        "setegid",
        "seteuid",
        "setgid",
        "setgroups",
        "setpgid",
        "setpgrp",
        "setpriority",
        "setregid",
        "setreuid",
        "setsid",
        "setuid",
        "spawnlp",
        "spawnlpe",
        "spawnvp",
        "spawnvpe",
        "statvfs",
        "sync",
        "sysconf",
        "sysconf_names",
        "tcgetpgrp",
        "tcsetpgrp",
        "ttyname",
        "uname",
        "wait",
        "wait3",
        "wait4",
        "writev",
    ]
if sys.platform != "win32" and sys.version_info >= (3, 13):
    __all__ += ["grantpt", "posix_openpt", "ptsname", "unlockpt"]
if sys.platform != "win32" and sys.version_info >= (3, 11):
    __all__ += ["login_tty"]
if sys.platform != "win32" and sys.version_info >= (3, 10):
    __all__ += ["O_FSYNC"]
if sys.platform != "darwin" and sys.platform != "win32":
    __all__ += [
        "POSIX_FADV_DONTNEED",
        "POSIX_FADV_NOREUSE",
        "POSIX_FADV_NORMAL",
        "POSIX_FADV_RANDOM",
        "POSIX_FADV_SEQUENTIAL",
        "POSIX_FADV_WILLNEED",
        "RWF_DSYNC",
        "RWF_HIPRI",
        "RWF_NOWAIT",
        "RWF_SYNC",
        "ST_APPEND",
        "ST_MANDLOCK",
        "ST_NOATIME",
        "ST_NODEV",
        "ST_NODIRATIME",
        "ST_NOEXEC",
        "ST_RELATIME",
        "ST_SYNCHRONOUS",
        "ST_WRITE",
        "fdatasync",
        "getresgid",
        "getresuid",
        "pipe2",
        "posix_fadvise",
        "posix_fallocate",
        "sched_getaffinity",
        "sched_getparam",
        "sched_getscheduler",
        "sched_param",
        "sched_rr_get_interval",
        "sched_setaffinity",
        "sched_setparam",
        "sched_setscheduler",
        "setresgid",
        "setresuid",
    ]
if sys.platform != "linux" and sys.platform != "win32":
    __all__ += ["O_EXLOCK", "O_SHLOCK", "chflags", "lchflags"]
if sys.platform != "linux" and sys.platform != "win32" and sys.version_info >= (3, 13):
    __all__ += ["O_EXEC", "O_SEARCH"]
if sys.platform != "darwin" or sys.version_info >= (3, 13):
    if sys.platform != "win32":
        __all__ += ["waitid", "waitid_result"]
if sys.platform != "win32" or sys.version_info >= (3, 13):
    __all__ += ["fchmod"]
    if sys.platform != "linux":
        __all__ += ["lchmod"]
if sys.platform != "win32" or sys.version_info >= (3, 12):
    __all__ += ["get_blocking", "set_blocking"]
if sys.platform != "win32" or sys.version_info >= (3, 11):
    __all__ += ["EX_OK"]

# This unnecessary alias is to work around various errors
path = _path

_T = TypeVar("_T")
_T1 = TypeVar("_T1")
_T2 = TypeVar("_T2")

# ----- os variables -----

error = OSError

supports_bytes_environ: bool

supports_dir_fd: set[Callable[..., Any]]
supports_fd: set[Callable[..., Any]]
supports_effective_ids: set[Callable[..., Any]]
supports_follow_symlinks: set[Callable[..., Any]]

if sys.platform != "win32":
    # Unix only
    PRIO_PROCESS: Final[int]
    PRIO_PGRP: Final[int]
    PRIO_USER: Final[int]

    F_LOCK: Final[int]
    F_TLOCK: Final[int]
    F_ULOCK: Final[int]
    F_TEST: Final[int]

    if sys.platform != "darwin":
        POSIX_FADV_NORMAL: Final[int]
        POSIX_FADV_SEQUENTIAL: Final[int]
        POSIX_FADV_RANDOM: Final[int]
        POSIX_FADV_NOREUSE: Final[int]
        POSIX_FADV_WILLNEED: Final[int]
        POSIX_FADV_DONTNEED: Final[int]

    if sys.platform != "linux" and sys.platform != "darwin":
        # In the os-module docs, these are marked as being available
        # on "Unix, not Emscripten, not WASI."
        # However, in the source code, a comment indicates they're "FreeBSD constants".
        # sys.platform could have one of many values on a FreeBSD Python build,
        # so the sys-module docs recommend doing `if sys.platform.startswith('freebsd')`
        # to detect FreeBSD builds. Unfortunately that would be too dynamic
        # for type checkers, however.
        SF_NODISKIO: Final[int]
        SF_MNOWAIT: Final[int]
        SF_SYNC: Final[int]

        if sys.version_info >= (3, 11):
            SF_NOCACHE: Final[int]

    if sys.platform == "linux":
        XATTR_SIZE_MAX: Final[int]
        XATTR_CREATE: Final[int]
        XATTR_REPLACE: Final[int]

    P_PID: Final[int]
    P_PGID: Final[int]
    P_ALL: Final[int]

    if sys.platform == "linux":
        P_PIDFD: Final[int]

    WEXITED: Final[int]
    WSTOPPED: Final[int]
    WNOWAIT: Final[int]

    CLD_EXITED: Final[int]
    CLD_DUMPED: Final[int]
    CLD_TRAPPED: Final[int]
    CLD_CONTINUED: Final[int]
    CLD_KILLED: Final[int]
    CLD_STOPPED: Final[int]

    SCHED_OTHER: Final[int]
    SCHED_FIFO: Final[int]
    SCHED_RR: Final[int]
    if sys.platform != "darwin" and sys.platform != "linux":
        SCHED_SPORADIC: Final[int]

if sys.platform == "linux":
    SCHED_BATCH: Final[int]
    SCHED_IDLE: Final[int]
    SCHED_RESET_ON_FORK: Final[int]

if sys.version_info >= (3, 14) and sys.platform == "linux":
    SCHED_DEADLINE: Final[int]
    SCHED_NORMAL: Final[int]

if sys.platform != "win32":
    RTLD_LAZY: Final[int]
    RTLD_NOW: Final[int]
    RTLD_GLOBAL: Final[int]
    RTLD_LOCAL: Final[int]
    RTLD_NODELETE: Final[int]
    RTLD_NOLOAD: Final[int]

if sys.platform == "linux":
    RTLD_DEEPBIND: Final[int]
    GRND_NONBLOCK: Final[int]
    GRND_RANDOM: Final[int]

if sys.platform == "darwin" and sys.version_info >= (3, 12):
    PRIO_DARWIN_BG: Final[int]
    PRIO_DARWIN_NONUI: Final[int]
    PRIO_DARWIN_PROCESS: Final[int]
    PRIO_DARWIN_THREAD: Final[int]

SEEK_SET: Final = 0
SEEK_CUR: Final = 1
SEEK_END: Final = 2
if sys.platform != "win32":
    SEEK_DATA: Final = 3
    SEEK_HOLE: Final = 4

O_RDONLY: Final[int]
O_WRONLY: Final[int]
O_RDWR: Final[int]
O_APPEND: Final[int]
O_CREAT: Final[int]
O_EXCL: Final[int]
O_TRUNC: Final[int]
if sys.platform == "win32":
    O_BINARY: Final[int]
    O_NOINHERIT: Final[int]
    O_SHORT_LIVED: Final[int]
    O_TEMPORARY: Final[int]
    O_RANDOM: Final[int]
    O_SEQUENTIAL: Final[int]
    O_TEXT: Final[int]

if sys.platform != "win32":
    O_DSYNC: Final[int]
    O_SYNC: Final[int]
    O_NDELAY: Final[int]
    O_NONBLOCK: Final[int]
    O_NOCTTY: Final[int]
    O_CLOEXEC: Final[int]
    O_ASYNC: Final[int]  # Gnu extension if in C library
    O_DIRECTORY: Final[int]  # Gnu extension if in C library
    O_NOFOLLOW: Final[int]  # Gnu extension if in C library
    O_ACCMODE: Final[int]  # TODO: when does this exist?

if sys.platform == "linux":
    O_RSYNC: Final[int]
    O_DIRECT: Final[int]  # Gnu extension if in C library
    O_NOATIME: Final[int]  # Gnu extension if in C library
    O_PATH: Final[int]  # Gnu extension if in C library
    O_TMPFILE: Final[int]  # Gnu extension if in C library
    O_LARGEFILE: Final[int]  # Gnu extension if in C library

if sys.platform != "linux" and sys.platform != "win32":
    O_SHLOCK: Final[int]
    O_EXLOCK: Final[int]

if sys.platform == "darwin" and sys.version_info >= (3, 10):
    O_EVTONLY: Final[int]
    O_NOFOLLOW_ANY: Final[int]
    O_SYMLINK: Final[int]

if sys.platform != "win32" and sys.version_info >= (3, 10):
    O_FSYNC: Final[int]

if sys.platform != "linux" and sys.platform != "win32" and sys.version_info >= (3, 13):
    O_EXEC: Final[int]
    O_SEARCH: Final[int]

if sys.platform != "win32" and sys.platform != "darwin":
    # posix, but apparently missing on macos
    ST_APPEND: Final[int]
    ST_MANDLOCK: Final[int]
    ST_NOATIME: Final[int]
    ST_NODEV: Final[int]
    ST_NODIRATIME: Final[int]
    ST_NOEXEC: Final[int]
    ST_RELATIME: Final[int]
    ST_SYNCHRONOUS: Final[int]
    ST_WRITE: Final[int]

if sys.platform != "win32":
    NGROUPS_MAX: Final[int]
    ST_NOSUID: Final[int]
    ST_RDONLY: Final[int]

linesep: Literal["\n", "\r\n"]
name: LiteralString

F_OK: Final = 0
R_OK: Final = 4
W_OK: Final = 2
X_OK: Final = 1

_EnvironCodeFunc: TypeAlias = Callable[[AnyStr], AnyStr]

class _Environ(MutableMapping[AnyStr, AnyStr], Generic[AnyStr]):
    encodekey: _EnvironCodeFunc[AnyStr]
    decodekey: _EnvironCodeFunc[AnyStr]
    encodevalue: _EnvironCodeFunc[AnyStr]
    decodevalue: _EnvironCodeFunc[AnyStr]
    def __init__(
        self,
        data: MutableMapping[AnyStr, AnyStr],
        encodekey: _EnvironCodeFunc[AnyStr],
        decodekey: _EnvironCodeFunc[AnyStr],
        encodevalue: _EnvironCodeFunc[AnyStr],
        decodevalue: _EnvironCodeFunc[AnyStr],
    ) -> None: ...
    def setdefault(self, key: AnyStr, value: AnyStr) -> AnyStr: ...
    def copy(self) -> dict[AnyStr, AnyStr]: ...
    def __delitem__(self, key: AnyStr) -> None: ...
    def __getitem__(self, key: AnyStr) -> AnyStr: ...
    def __setitem__(self, key: AnyStr, value: AnyStr) -> None: ...
    def __iter__(self) -> Iterator[AnyStr]: ...
    def __len__(self) -> int: ...
    def __or__(self, other: Mapping[_T1, _T2]) -> dict[AnyStr | _T1, AnyStr | _T2]: ...
    def __ror__(self, other: Mapping[_T1, _T2]) -> dict[AnyStr | _T1, AnyStr | _T2]: ...
    # We use @overload instead of a Union for reasons similar to those given for
    # overloading MutableMapping.update in stdlib/typing.pyi
    # The type: ignore is needed due to incompatible __or__/__ior__ signatures
    @overload  # type: ignore[misc]
    def __ior__(self, other: Mapping[AnyStr, AnyStr]) -> Self: ...
    @overload
    def __ior__(self, other: Iterable[tuple[AnyStr, AnyStr]]) -> Self: ...

environ: _Environ[str]
if sys.platform != "win32":
    environb: _Environ[bytes]

if sys.version_info >= (3, 14):
    def reload_environ() -> None: ...

if sys.version_info >= (3, 11) or sys.platform != "win32":
    EX_OK: Final[int]

if sys.platform != "win32":
    confstr_names: dict[str, int]
    pathconf_names: dict[str, int]
    sysconf_names: dict[str, int]

    EX_USAGE: Final[int]
    EX_DATAERR: Final[int]
    EX_NOINPUT: Final[int]
    EX_NOUSER: Final[int]
    EX_NOHOST: Final[int]
    EX_UNAVAILABLE: Final[int]
    EX_SOFTWARE: Final[int]
    EX_OSERR: Final[int]
    EX_OSFILE: Final[int]
    EX_CANTCREAT: Final[int]
    EX_IOERR: Final[int]
    EX_TEMPFAIL: Final[int]
    EX_PROTOCOL: Final[int]
    EX_NOPERM: Final[int]
    EX_CONFIG: Final[int]

# Exists on some Unix platforms, e.g. Solaris.
if sys.platform != "win32" and sys.platform != "darwin" and sys.platform != "linux":
    EX_NOTFOUND: Final[int]

P_NOWAIT: Final[int]
P_NOWAITO: Final[int]
P_WAIT: Final[int]
if sys.platform == "win32":
    P_DETACH: Final[int]
    P_OVERLAY: Final[int]

# wait()/waitpid() options
if sys.platform != "win32":
    WNOHANG: Final[int]  # Unix only
    WCONTINUED: Final[int]  # some Unix systems
    WUNTRACED: Final[int]  # Unix only

TMP_MAX: Final[int]  # Undocumented, but used by tempfile

# ----- os classes (structures) -----
@final
class stat_result(structseq[float], tuple[int, int, int, int, int, int, int, float, float, float]):
    """stat_result: Result from stat, fstat, or lstat.

    This object may be accessed either as a tuple of
      (mode, ino, dev, nlink, uid, gid, size, atime, mtime, ctime)
    or via the attributes st_mode, st_ino, st_dev, st_nlink, st_uid, and so on.

    Posix/windows: If your platform supports st_blksize, st_blocks, st_rdev,
    or st_flags, they are available as attributes only.

    See os.stat for more information.
    """

    # The constructor of this class takes an iterable of variable length (though it must be at least 10).
    #
    # However, this class behaves like a tuple of 10 elements,
    # no matter how long the iterable supplied to the constructor is.
    # https://github.com/python/typeshed/pull/6560#discussion_r767162532
    #
    # The 10 elements always present are st_mode, st_ino, st_dev, st_nlink,
    # st_uid, st_gid, st_size, st_atime, st_mtime, st_ctime.
    #
    # More items may be added at the end by some implementations.
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("st_mode", "st_ino", "st_dev", "st_nlink", "st_uid", "st_gid", "st_size")

    @property
    def st_mode(self) -> int:  # protection bits,
        """protection bits"""

    @property
    def st_ino(self) -> int:  # inode number,
        """inode"""

    @property
    def st_dev(self) -> int:  # device,
        """device"""

    @property
    def st_nlink(self) -> int:  # number of hard links,
        """number of hard links"""

    @property
    def st_uid(self) -> int:  # user id of owner,
        """user ID of owner"""

    @property
    def st_gid(self) -> int:  # group id of owner,
        """group ID of owner"""

    @property
    def st_size(self) -> int:  # size of file, in bytes,
        """total size, in bytes"""

    @property
    def st_atime(self) -> float:  # time of most recent access,
        """time of last access"""

    @property
    def st_mtime(self) -> float:  # time of most recent content modification,
        """time of last modification"""
    # platform dependent (time of most recent metadata change on Unix, or the time of creation on Windows)
    if sys.version_info >= (3, 12) and sys.platform == "win32":
        @property
        @deprecated(
            """\
Use st_birthtime instead to retrieve the file creation time. \
In the future, this property will contain the last metadata change time."""
        )
        def st_ctime(self) -> float:
            """time of last change"""
    else:
        @property
        def st_ctime(self) -> float:
            """time of last change"""

    @property
    def st_atime_ns(self) -> int:  # time of most recent access, in nanoseconds
        """time of last access in nanoseconds"""

    @property
    def st_mtime_ns(self) -> int:  # time of most recent content modification in nanoseconds
        """time of last modification in nanoseconds"""
    # platform dependent (time of most recent metadata change on Unix, or the time of creation on Windows) in nanoseconds
    @property
    def st_ctime_ns(self) -> int:
        """time of last change in nanoseconds"""
    if sys.platform == "win32":
        @property
        def st_file_attributes(self) -> int:
            """Windows file attribute bits"""

        @property
        def st_reparse_tag(self) -> int:
            """Windows reparse tag"""
        if sys.version_info >= (3, 12):
            @property
            def st_birthtime(self) -> float:  # time of file creation in seconds
                """time of creation"""

            @property
            def st_birthtime_ns(self) -> int:  # time of file creation in nanoseconds
                """time of creation in nanoseconds"""
    else:
        @property
        def st_blocks(self) -> int:  # number of blocks allocated for file
            """number of blocks allocated"""

        @property
        def st_blksize(self) -> int:  # filesystem blocksize
            """blocksize for filesystem I/O"""

        @property
        def st_rdev(self) -> int:  # type of device if an inode device
            """device type (if inode device)"""
        if sys.platform != "linux":
            # These properties are available on MacOS, but not Ubuntu.
            # On other Unix systems (such as FreeBSD), the following attributes may be
            # available (but may be only filled out if root tries to use them):
            @property
            def st_gen(self) -> int:  # file generation number
                """generation number"""

            @property
            def st_birthtime(self) -> float:  # time of file creation in seconds
                """time of creation"""
    if sys.platform == "darwin":
        @property
        def st_flags(self) -> int:  # user defined flags for file
            """user defined flags for file"""
    # Attributes documented as sometimes appearing, but deliberately omitted from the stub: `st_creator`, `st_rsize`, `st_type`.
    # See https://github.com/python/typeshed/pull/6560#issuecomment-991253327

# mypy and pyright object to this being both ABC and Protocol.
# At runtime it inherits from ABC and is not a Protocol, but it will be
# on the allowlist for use as a Protocol starting in 3.14.
@runtime_checkable
class PathLike(ABC, Protocol[AnyStr_co]):  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
    """Abstract base class for implementing the file system path protocol."""

    __slots__ = ()
    @abstractmethod
    def __fspath__(self) -> AnyStr_co:
        """Return the file system path representation of the object."""

@overload
def listdir(path: StrPath | None = None) -> list[str]:
    """Return a list containing the names of the files in the directory.

path can be specified as either str, bytes, or a path-like object.  If path is bytes,
  the filenames returned will also be bytes; in all other circumstances
  the filenames returned will be str.
If path is None, uses the path='.'.
On some platforms, path may also be specified as an open file descriptor;\\
  the file descriptor must refer to a directory.
  If this functionality is unavailable, using it raises NotImplementedError.

The list is in arbitrary order.  It does not include the special
entries '.' and '..' even if they are present in the directory.
"""

@overload
def listdir(path: BytesPath) -> list[bytes]: ...
@overload
def listdir(path: int) -> list[str]: ...
@final
class DirEntry(Generic[AnyStr]):
    # This is what the scandir iterator yields
    # The constructor is hidden

    @property
    def name(self) -> AnyStr:
        """the entry's base filename, relative to scandir() "path" argument"""

    @property
    def path(self) -> AnyStr:
        """the entry's full path name; equivalent to os.path.join(scandir_path, entry.name)"""

    def inode(self) -> int:
        """Return inode of the entry; cached per entry."""

    def is_dir(self, *, follow_symlinks: bool = True) -> bool:
        """Return True if the entry is a directory; cached per entry."""

    def is_file(self, *, follow_symlinks: bool = True) -> bool:
        """Return True if the entry is a file; cached per entry."""

    def is_symlink(self) -> bool:
        """Return True if the entry is a symbolic link; cached per entry."""

    def stat(self, *, follow_symlinks: bool = True) -> stat_result:
        """Return stat_result object for the entry; cached per entry."""

    def __fspath__(self) -> AnyStr:
        """Returns the path for the entry."""

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""
    if sys.version_info >= (3, 12):
        def is_junction(self) -> bool:
            """Return True if the entry is a junction; cached per entry."""

@final
class statvfs_result(structseq[int], tuple[int, int, int, int, int, int, int, int, int, int, int]):
    """statvfs_result: Result from statvfs or fstatvfs.

    This object may be accessed either as a tuple of
      (bsize, frsize, blocks, bfree, bavail, files, ffree, favail, flag, namemax),
    or via the attributes f_bsize, f_frsize, f_blocks, f_bfree, and so on.

    See os.statvfs for more information.
    """

    if sys.version_info >= (3, 10):
        __match_args__: Final = (
            "f_bsize",
            "f_frsize",
            "f_blocks",
            "f_bfree",
            "f_bavail",
            "f_files",
            "f_ffree",
            "f_favail",
            "f_flag",
            "f_namemax",
        )

    @property
    def f_bsize(self) -> int: ...
    @property
    def f_frsize(self) -> int: ...
    @property
    def f_blocks(self) -> int: ...
    @property
    def f_bfree(self) -> int: ...
    @property
    def f_bavail(self) -> int: ...
    @property
    def f_files(self) -> int: ...
    @property
    def f_ffree(self) -> int: ...
    @property
    def f_favail(self) -> int: ...
    @property
    def f_flag(self) -> int: ...
    @property
    def f_namemax(self) -> int: ...
    @property
    def f_fsid(self) -> int: ...

# ----- os function stubs -----
def fsencode(filename: StrOrBytesPath) -> bytes:
    """Encode filename (an os.PathLike, bytes, or str) to the filesystem
    encoding with 'surrogateescape' error handler, return bytes unchanged.
    On Windows, use 'strict' error handler if the file system encoding is
    'mbcs' (which is the default encoding).
    """

def fsdecode(filename: StrOrBytesPath) -> str:
    """Decode filename (an os.PathLike, bytes, or str) from the filesystem
    encoding with 'surrogateescape' error handler, return str unchanged. On
    Windows, use 'strict' error handler if the file system encoding is
    'mbcs' (which is the default encoding).
    """

@overload
def fspath(path: str) -> str:
    """Return the file system path representation of the object.

    If the object is str or bytes, then allow it to pass through as-is. If the
    object defines __fspath__(), then return the result of that method. All other
    types raise a TypeError.
    """

@overload
def fspath(path: bytes) -> bytes: ...
@overload
def fspath(path: PathLike[AnyStr]) -> AnyStr: ...
def get_exec_path(env: Mapping[str, str] | None = None) -> list[str]:
    """Returns the sequence of directories that will be searched for the
    named executable (similar to a shell) when launching a process.

    *env* must be an environment variable dict or None.  If *env* is None,
    os.environ will be used.
    """

def getlogin() -> str:
    """Return the actual login name."""

def getpid() -> int:
    """Return the current process id."""

def getppid() -> int:
    """Return the parent's process id.

    If the parent process has already exited, Windows machines will still
    return its id; others systems will return the id of the 'init' process (1).
    """

def strerror(code: int, /) -> str:
    """Translate an error code to a message string."""

def umask(mask: int, /) -> int:
    """Set the current numeric umask and return the previous umask."""

@final
class uname_result(structseq[str], tuple[str, str, str, str, str]):
    """uname_result: Result from os.uname().

    This object may be accessed either as a tuple of
      (sysname, nodename, release, version, machine),
    or via the attributes sysname, nodename, release, version, and machine.

    See os.uname for more information.
    """

    if sys.version_info >= (3, 10):
        __match_args__: Final = ("sysname", "nodename", "release", "version", "machine")

    @property
    def sysname(self) -> str:
        """operating system name"""

    @property
    def nodename(self) -> str:
        """name of machine on network (implementation-defined)"""

    @property
    def release(self) -> str:
        """operating system release"""

    @property
    def version(self) -> str:
        """operating system version"""

    @property
    def machine(self) -> str:
        """hardware identifier"""

if sys.platform != "win32":
    def ctermid() -> str:
        """Return the name of the controlling terminal for this process."""

    def getegid() -> int:
        """Return the current process's effective group id."""

    def geteuid() -> int:
        """Return the current process's effective user id."""

    def getgid() -> int:
        """Return the current process's group id."""

    def getgrouplist(user: str, group: int, /) -> list[int]:
        """Returns a list of groups to which a user belongs.

        user
          username to lookup
        group
          base group id of the user
        """

    def getgroups() -> list[int]:  # Unix only, behaves differently on Mac
        """Return list of supplemental group IDs for the process."""

    def initgroups(username: str, gid: int, /) -> None:
        """Initialize the group access list.

        Call the system initgroups() to initialize the group access list with all of
        the groups of which the specified username is a member, plus the specified
        group id.
        """

    def getpgid(pid: int) -> int:
        """Call the system call getpgid(), and return the result."""

    def getpgrp() -> int:
        """Return the current process group id."""

    def getpriority(which: int, who: int) -> int:
        """Return program scheduling priority."""

    def setpriority(which: int, who: int, priority: int) -> None:
        """Set program scheduling priority."""
    if sys.platform != "darwin":
        def getresuid() -> tuple[int, int, int]:
            """Return a tuple of the current process's real, effective, and saved user ids."""

        def getresgid() -> tuple[int, int, int]:
            """Return a tuple of the current process's real, effective, and saved group ids."""

    def getuid() -> int:
        """Return the current process's user id."""

    def setegid(egid: int, /) -> None:
        """Set the current process's effective group id."""

    def seteuid(euid: int, /) -> None:
        """Set the current process's effective user id."""

    def setgid(gid: int, /) -> None:
        """Set the current process's group id."""

    def setgroups(groups: Sequence[int], /) -> None:
        """Set the groups of the current process to list."""

    def setpgrp() -> None:
        """Make the current process the leader of its process group."""

    def setpgid(pid: int, pgrp: int, /) -> None:
        """Call the system call setpgid(pid, pgrp)."""

    def setregid(rgid: int, egid: int, /) -> None:
        """Set the current process's real and effective group ids."""
    if sys.platform != "darwin":
        def setresgid(rgid: int, egid: int, sgid: int, /) -> None:
            """Set the current process's real, effective, and saved group ids."""

        def setresuid(ruid: int, euid: int, suid: int, /) -> None:
            """Set the current process's real, effective, and saved user ids."""

    def setreuid(ruid: int, euid: int, /) -> None:
        """Set the current process's real and effective user ids."""

    def getsid(pid: int, /) -> int:
        """Call the system call getsid(pid) and return the result."""

    def setsid() -> None:
        """Call the system call setsid()."""

    def setuid(uid: int, /) -> None:
        """Set the current process's user id."""

    def uname() -> uname_result:
        """Return an object identifying the current operating system.

        The object behaves like a named tuple with the following fields:
          (sysname, nodename, release, version, machine)
        """

@overload
def getenv(key: str) -> str | None:
    """Get an environment variable, return None if it doesn't exist.
    The optional second argument can specify an alternate default.
    key, default and the result are str.
    """

@overload
def getenv(key: str, default: _T) -> str | _T: ...

if sys.platform != "win32":
    @overload
    def getenvb(key: bytes) -> bytes | None:
        """Get an environment variable, return None if it doesn't exist.
        The optional second argument can specify an alternate default.
        key, default and the result are bytes.
        """

    @overload
    def getenvb(key: bytes, default: _T) -> bytes | _T: ...
    def putenv(name: StrOrBytesPath, value: StrOrBytesPath, /) -> None:
        """Change or add an environment variable."""

    def unsetenv(name: StrOrBytesPath, /) -> None:
        """Delete an environment variable."""

else:
    def putenv(name: str, value: str, /) -> None:
        """Change or add an environment variable."""

    def unsetenv(name: str, /) -> None:
        """Delete an environment variable."""

_Opener: TypeAlias = Callable[[str, int], int]

@overload
def fdopen(
    fd: int,
    mode: OpenTextMode = "r",
    buffering: int = -1,
    encoding: str | None = None,
    errors: str | None = ...,
    newline: str | None = ...,
    closefd: bool = ...,
    opener: _Opener | None = ...,
) -> TextIOWrapper: ...
@overload
def fdopen(
    fd: int,
    mode: OpenBinaryMode,
    buffering: Literal[0],
    encoding: None = None,
    errors: None = None,
    newline: None = None,
    closefd: bool = ...,
    opener: _Opener | None = ...,
) -> FileIO: ...
@overload
def fdopen(
    fd: int,
    mode: OpenBinaryModeUpdating,
    buffering: Literal[-1, 1] = -1,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
    closefd: bool = ...,
    opener: _Opener | None = ...,
) -> BufferedRandom: ...
@overload
def fdopen(
    fd: int,
    mode: OpenBinaryModeWriting,
    buffering: Literal[-1, 1] = -1,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
    closefd: bool = ...,
    opener: _Opener | None = ...,
) -> BufferedWriter: ...
@overload
def fdopen(
    fd: int,
    mode: OpenBinaryModeReading,
    buffering: Literal[-1, 1] = -1,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
    closefd: bool = ...,
    opener: _Opener | None = ...,
) -> BufferedReader: ...
@overload
def fdopen(
    fd: int,
    mode: OpenBinaryMode,
    buffering: int = -1,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
    closefd: bool = ...,
    opener: _Opener | None = ...,
) -> BinaryIO: ...
@overload
def fdopen(
    fd: int,
    mode: str,
    buffering: int = -1,
    encoding: str | None = None,
    errors: str | None = ...,
    newline: str | None = ...,
    closefd: bool = ...,
    opener: _Opener | None = ...,
) -> IO[Any]: ...
def close(fd: int) -> None:
    """Close a file descriptor."""

def closerange(fd_low: int, fd_high: int, /) -> None:
    """Closes all file descriptors in [fd_low, fd_high), ignoring errors."""

def device_encoding(fd: int) -> str | None:
    """Return a string describing the encoding of a terminal's file descriptor.

    The file descriptor must be attached to a terminal.
    If the device is not a terminal, return None.
    """

def dup(fd: int, /) -> int:
    """Return a duplicate of a file descriptor."""

def dup2(fd: int, fd2: int, inheritable: bool = True) -> int:
    """Duplicate file descriptor."""

def fstat(fd: int) -> stat_result:
    """Perform a stat system call on the given file descriptor.

    Like stat(), but for an open file descriptor.
    Equivalent to os.stat(fd).
    """

def ftruncate(fd: int, length: int, /) -> None:
    """Truncate a file, specified by file descriptor, to a specific length."""

def fsync(fd: FileDescriptorLike) -> None:
    """Force write of fd to disk."""

def isatty(fd: int, /) -> bool:
    """Return True if the fd is connected to a terminal.

    Return True if the file descriptor is an open file descriptor
    connected to the slave end of a terminal.
    """

if sys.platform != "win32" and sys.version_info >= (3, 11):
    def login_tty(fd: int, /) -> None:
        """Prepare the tty of which fd is a file descriptor for a new login session.

        Make the calling process a session leader; make the tty the
        controlling tty, the stdin, the stdout, and the stderr of the
        calling process; close fd.
        """

if sys.version_info >= (3, 11):
    def lseek(fd: int, position: int, whence: int, /) -> int:
        """Set the position of a file descriptor.  Return the new position.

          fd
            An open file descriptor, as returned by os.open().
          position
            Position, interpreted relative to 'whence'.
          whence
            The relative position to seek from. Valid values are:
            - SEEK_SET: seek from the start of the file.
            - SEEK_CUR: seek from the current file position.
            - SEEK_END: seek from the end of the file.

        The return value is the number of bytes relative to the beginning of the file.
        """

else:
    def lseek(fd: int, position: int, how: int, /) -> int:
        """Set the position of a file descriptor.  Return the new position.

        Return the new cursor position in number of bytes
        relative to the beginning of the file.
        """

def open(path: StrOrBytesPath, flags: int, mode: int = 0o777, *, dir_fd: int | None = None) -> int:
    """Open a file for low level IO.  Returns a file descriptor (integer).

    If dir_fd is not None, it should be a file descriptor open to a directory,
      and path should be relative; path will then be relative to that directory.
    dir_fd may not be implemented on your platform.
      If it is unavailable, using it will raise a NotImplementedError.
    """

def pipe() -> tuple[int, int]:
    """Create a pipe.

    Returns a tuple of two file descriptors:
      (read_fd, write_fd)
    """

def read(fd: int, length: int, /) -> bytes:
    """Read from a file descriptor.  Returns a bytes object."""

if sys.version_info >= (3, 12) or sys.platform != "win32":
    def get_blocking(fd: int, /) -> bool:
        """Get the blocking mode of the file descriptor.

        Return False if the O_NONBLOCK flag is set, True if the flag is cleared.
        """

    def set_blocking(fd: int, blocking: bool, /) -> None:
        """Set the blocking mode of the specified file descriptor.

        Set the O_NONBLOCK flag if blocking is False,
        clear the O_NONBLOCK flag otherwise.
        """

if sys.platform != "win32":
    def fchown(fd: int, uid: int, gid: int) -> None:
        """Change the owner and group id of the file specified by file descriptor.

        Equivalent to os.chown(fd, uid, gid).
        """

    def fpathconf(fd: int, name: str | int, /) -> int:
        """Return the configuration limit name for the file descriptor fd.

        If there is no limit, return -1.
        """

    def fstatvfs(fd: int, /) -> statvfs_result:
        """Perform an fstatvfs system call on the given fd.

        Equivalent to statvfs(fd).
        """

    def lockf(fd: int, command: int, length: int, /) -> None:
        """Apply, test or remove a POSIX lock on an open file descriptor.

        fd
          An open file descriptor.
        command
          One of F_LOCK, F_TLOCK, F_ULOCK or F_TEST.
        length
          The number of bytes to lock, starting at the current position.
        """

    def openpty() -> tuple[int, int]:  # some flavors of Unix
        """Open a pseudo-terminal.

        Return a tuple of (master_fd, slave_fd) containing open file descriptors
        for both the master and slave ends.
        """
    if sys.platform != "darwin":
        def fdatasync(fd: FileDescriptorLike) -> None:
            """Force write of fd to disk without forcing update of metadata."""

        def pipe2(flags: int, /) -> tuple[int, int]:  # some flavors of Unix
            """Create a pipe with flags set atomically.

            Returns a tuple of two file descriptors:
              (read_fd, write_fd)

            flags can be constructed by ORing together one or more of these values:
            O_NONBLOCK, O_CLOEXEC.
            """

        def posix_fallocate(fd: int, offset: int, length: int, /) -> None:
            """Ensure a file has allocated at least a particular number of bytes on disk.

            Ensure that the file specified by fd encompasses a range of bytes
            starting at offset bytes from the beginning and continuing for length bytes.
            """

        def posix_fadvise(fd: int, offset: int, length: int, advice: int, /) -> None:
            """Announce an intention to access data in a specific pattern.

            Announce an intention to access data in a specific pattern, thus allowing
            the kernel to make optimizations.
            The advice applies to the region of the file specified by fd starting at
            offset and continuing for length bytes.
            advice is one of POSIX_FADV_NORMAL, POSIX_FADV_SEQUENTIAL,
            POSIX_FADV_RANDOM, POSIX_FADV_NOREUSE, POSIX_FADV_WILLNEED, or
            POSIX_FADV_DONTNEED.
            """

    def pread(fd: int, length: int, offset: int, /) -> bytes:
        """Read a number of bytes from a file descriptor starting at a particular offset.

        Read length bytes from file descriptor fd, starting at offset bytes from
        the beginning of the file.  The file offset remains unchanged.
        """

    def pwrite(fd: int, buffer: ReadableBuffer, offset: int, /) -> int:
        """Write bytes to a file descriptor starting at a particular offset.

        Write buffer to fd, starting at offset bytes from the beginning of
        the file.  Returns the number of bytes written.  Does not change the
        current file offset.
        """
    # In CI, stubtest sometimes reports that these are available on MacOS, sometimes not
    def preadv(fd: int, buffers: SupportsLenAndGetItem[WriteableBuffer], offset: int, flags: int = 0, /) -> int:
        """Reads from a file descriptor into a number of mutable bytes-like objects.

        Combines the functionality of readv() and pread(). As readv(), it will
        transfer data into each buffer until it is full and then move on to the next
        buffer in the sequence to hold the rest of the data. Its fourth argument,
        specifies the file offset at which the input operation is to be performed. It
        will return the total number of bytes read (which can be less than the total
        capacity of all the objects).

        The flags argument contains a bitwise OR of zero or more of the following flags:

        - RWF_HIPRI
        - RWF_NOWAIT

        Using non-zero flags requires Linux 4.6 or newer.
        """

    def pwritev(fd: int, buffers: SupportsLenAndGetItem[ReadableBuffer], offset: int, flags: int = 0, /) -> int:
        """Writes the contents of bytes-like objects to a file descriptor at a given offset.

        Combines the functionality of writev() and pwrite(). All buffers must be a sequence
        of bytes-like objects. Buffers are processed in array order. Entire contents of first
        buffer is written before proceeding to second, and so on. The operating system may
        set a limit (sysconf() value SC_IOV_MAX) on the number of buffers that can be used.
        This function writes the contents of each object to the file descriptor and returns
        the total number of bytes written.

        The flags argument contains a bitwise OR of zero or more of the following flags:

        - RWF_DSYNC
        - RWF_SYNC
        - RWF_APPEND

        Using non-zero flags requires Linux 4.7 or newer.
        """
    if sys.platform != "darwin":
        if sys.version_info >= (3, 10):
            RWF_APPEND: Final[int]  # docs say available on 3.7+, stubtest says otherwise
        RWF_DSYNC: Final[int]
        RWF_SYNC: Final[int]
        RWF_HIPRI: Final[int]
        RWF_NOWAIT: Final[int]

    if sys.platform == "linux":
        def sendfile(out_fd: FileDescriptor, in_fd: FileDescriptor, offset: int | None, count: int) -> int:
            """Copy count bytes from file descriptor in_fd to file descriptor out_fd."""
    else:
        def sendfile(
            out_fd: FileDescriptor,
            in_fd: FileDescriptor,
            offset: int,
            count: int,
            headers: Sequence[ReadableBuffer] = (),
            trailers: Sequence[ReadableBuffer] = (),
            flags: int = 0,
        ) -> int:  # FreeBSD and Mac OS X only
            """Copy count bytes from file descriptor in_fd to file descriptor out_fd."""

    def readv(fd: int, buffers: SupportsLenAndGetItem[WriteableBuffer], /) -> int:
        """Read from a file descriptor fd into an iterable of buffers.

        The buffers should be mutable buffers accepting bytes.
        readv will transfer data into each buffer until it is full
        and then move on to the next buffer in the sequence to hold
        the rest of the data.

        readv returns the total number of bytes read,
        which may be less than the total capacity of all the buffers.
        """

    def writev(fd: int, buffers: SupportsLenAndGetItem[ReadableBuffer], /) -> int:
        """Iterate over buffers, and write the contents of each to a file descriptor.

        Returns the total number of bytes written.
        buffers must be a sequence of bytes-like objects.
        """

if sys.version_info >= (3, 14):
    def readinto(fd: int, buffer: ReadableBuffer, /) -> int:
        """Read into a buffer object from a file descriptor.

        The buffer should be mutable and bytes-like. On success, returns the number of
        bytes read. Less bytes may be read than the size of the buffer. The underlying
        system call will be retried when interrupted by a signal, unless the signal
        handler raises an exception. Other errors will not be retried and an error will
        be raised.

        Returns 0 if *fd* is at end of file or if the provided *buffer* has length 0
        (which can be used to check for errors without reading data). Never returns
        negative.
        """

@final
class terminal_size(structseq[int], tuple[int, int]):
    """A tuple of (columns, lines) for holding terminal window size"""

    if sys.version_info >= (3, 10):
        __match_args__: Final = ("columns", "lines")

    @property
    def columns(self) -> int:
        """width of the terminal window in characters"""

    @property
    def lines(self) -> int:
        """height of the terminal window in characters"""

def get_terminal_size(fd: int = ..., /) -> terminal_size:
    """Return the size of the terminal window as (columns, lines).

    The optional argument fd (default standard output) specifies
    which file descriptor should be queried.

    If the file descriptor is not connected to a terminal, an OSError
    is thrown.

    This function will only be defined if an implementation is
    available for this system.

    shutil.get_terminal_size is the high-level function which should
    normally be used, os.get_terminal_size is the low-level implementation.
    """

def get_inheritable(fd: int, /) -> bool:
    """Get the close-on-exe flag of the specified file descriptor."""

def set_inheritable(fd: int, inheritable: bool, /) -> None:
    """Set the inheritable flag of the specified file descriptor."""

if sys.platform == "win32":
    def get_handle_inheritable(handle: int, /) -> bool:
        """Get the close-on-exe flag of the specified file descriptor."""

    def set_handle_inheritable(handle: int, inheritable: bool, /) -> None:
        """Set the inheritable flag of the specified handle."""

if sys.platform != "win32":
    # Unix only
    def tcgetpgrp(fd: int, /) -> int:
        """Return the process group associated with the terminal specified by fd."""

    def tcsetpgrp(fd: int, pgid: int, /) -> None:
        """Set the process group associated with the terminal specified by fd."""

    def ttyname(fd: int, /) -> str:
        """Return the name of the terminal device connected to 'fd'.

        fd
          Integer file descriptor handle.
        """

def write(fd: int, data: ReadableBuffer, /) -> int:
    """Write a bytes object to a file descriptor."""

def access(
    path: FileDescriptorOrPath, mode: int, *, dir_fd: int | None = None, effective_ids: bool = False, follow_symlinks: bool = True
) -> bool:
    """Use the real uid/gid to test for access to a path.

      path
        Path to be tested; can be string, bytes, or a path-like object.
      mode
        Operating-system mode bitfield.  Can be F_OK to test existence,
        or the inclusive-OR of R_OK, W_OK, and X_OK.
      dir_fd
        If not None, it should be a file descriptor open to a directory,
        and path should be relative; path will then be relative to that
        directory.
      effective_ids
        If True, access will use the effective uid/gid instead of
        the real uid/gid.
      follow_symlinks
        If False, and the last element of the path is a symbolic link,
        access will examine the symbolic link itself instead of the file
        the link points to.

    dir_fd, effective_ids, and follow_symlinks may not be implemented
      on your platform.  If they are unavailable, using them will raise a
      NotImplementedError.

    Note that most operations will use the effective uid/gid, therefore this
      routine can be used in a suid/sgid environment to test if the invoking user
      has the specified access to the path.
    """

def chdir(path: FileDescriptorOrPath) -> None:
    """Change the current working directory to the specified path.

    path may always be specified as a string.
    On some platforms, path may also be specified as an open file descriptor.
    If this functionality is unavailable, using it raises an exception.
    """

if sys.platform != "win32":
    def fchdir(fd: FileDescriptorLike) -> None:
        """Change to the directory of the given file descriptor.

        fd must be opened on a directory, not a file.
        Equivalent to os.chdir(fd).
        """

def getcwd() -> str:
    """Return a unicode string representing the current working directory."""

def getcwdb() -> bytes:
    """Return a bytes string representing the current working directory."""

def chmod(path: FileDescriptorOrPath, mode: int, *, dir_fd: int | None = None, follow_symlinks: bool = True) -> None:
    """Change the access permissions of a file.

      path
        Path to be modified.  May always be specified as a str, bytes, or a path-like object.
        On some platforms, path may also be specified as an open file descriptor.
        If this functionality is unavailable, using it raises an exception.
      mode
        Operating-system mode bitfield.
        Be careful when using number literals for *mode*. The conventional UNIX notation for
        numeric modes uses an octal base, which needs to be indicated with a ``0o`` prefix in
        Python.
      dir_fd
        If not None, it should be a file descriptor open to a directory,
        and path should be relative; path will then be relative to that
        directory.
      follow_symlinks
        If False, and the last element of the path is a symbolic link,
        chmod will modify the symbolic link itself instead of the file
        the link points to.

    It is an error to use dir_fd or follow_symlinks when specifying path as
      an open file descriptor.
    dir_fd and follow_symlinks may not be implemented on your platform.
      If they are unavailable, using them will raise a NotImplementedError.
    """

if sys.platform != "win32" and sys.platform != "linux":
    def chflags(path: StrOrBytesPath, flags: int, follow_symlinks: bool = True) -> None:  # some flavors of Unix
        """Set file flags.

        If follow_symlinks is False, and the last element of the path is a symbolic
          link, chflags will change flags on the symbolic link itself instead of the
          file the link points to.
        follow_symlinks may not be implemented on your platform.  If it is
        unavailable, using it will raise a NotImplementedError.
        """

    def lchflags(path: StrOrBytesPath, flags: int) -> None:
        """Set file flags.

        This function will not follow symbolic links.
        Equivalent to chflags(path, flags, follow_symlinks=False).
        """

if sys.platform != "win32":
    def chroot(path: StrOrBytesPath) -> None:
        """Change root directory to path."""

    def chown(path: FileDescriptorOrPath, uid: int, gid: int, *, dir_fd: int | None = None, follow_symlinks: bool = True) -> None:
        """Change the owner and group id of path to the numeric uid and gid.\\

  path
    Path to be examined; can be string, bytes, a path-like object, or open-file-descriptor int.
  dir_fd
    If not None, it should be a file descriptor open to a directory,
    and path should be relative; path will then be relative to that
    directory.
  follow_symlinks
    If False, and the last element of the path is a symbolic link,
    stat will examine the symbolic link itself instead of the file
    the link points to.

path may always be specified as a string.
On some platforms, path may also be specified as an open file descriptor.
  If this functionality is unavailable, using it raises an exception.
If dir_fd is not None, it should be a file descriptor open to a directory,
  and path should be relative; path will then be relative to that directory.
If follow_symlinks is False, and the last element of the path is a symbolic
  link, chown will modify the symbolic link itself instead of the file the
  link points to.
It is an error to use dir_fd or follow_symlinks when specifying path as
  an open file descriptor.
dir_fd and follow_symlinks may not be implemented on your platform.
  If they are unavailable, using them will raise a NotImplementedError.
"""

    def lchown(path: StrOrBytesPath, uid: int, gid: int) -> None:
        """Change the owner and group id of path to the numeric uid and gid.

        This function will not follow symbolic links.
        Equivalent to os.chown(path, uid, gid, follow_symlinks=False).
        """

def link(
    src: StrOrBytesPath,
    dst: StrOrBytesPath,
    *,
    src_dir_fd: int | None = None,
    dst_dir_fd: int | None = None,
    follow_symlinks: bool = True,
) -> None:
    """Create a hard link to a file.

    If either src_dir_fd or dst_dir_fd is not None, it should be a file
      descriptor open to a directory, and the respective path string (src or dst)
      should be relative; the path will then be relative to that directory.
    If follow_symlinks is False, and the last element of src is a symbolic
      link, link will create a link to the symbolic link itself instead of the
      file the link points to.
    src_dir_fd, dst_dir_fd, and follow_symlinks may not be implemented on your
      platform.  If they are unavailable, using them will raise a
      NotImplementedError.
    """

def lstat(path: StrOrBytesPath, *, dir_fd: int | None = None) -> stat_result:
    """Perform a stat system call on the given path, without following symbolic links.

    Like stat(), but do not follow symbolic links.
    Equivalent to stat(path, follow_symlinks=False).
    """

def mkdir(path: StrOrBytesPath, mode: int = 0o777, *, dir_fd: int | None = None) -> None:
    """Create a directory.

    If dir_fd is not None, it should be a file descriptor open to a directory,
      and path should be relative; path will then be relative to that directory.
    dir_fd may not be implemented on your platform.
      If it is unavailable, using it will raise a NotImplementedError.

    The mode argument is ignored on Windows. Where it is used, the current umask
    value is first masked out.
    """

if sys.platform != "win32":
    def mkfifo(path: StrOrBytesPath, mode: int = 0o666, *, dir_fd: int | None = None) -> None:  # Unix only
        """Create a "fifo" (a POSIX named pipe).

        If dir_fd is not None, it should be a file descriptor open to a directory,
          and path should be relative; path will then be relative to that directory.
        dir_fd may not be implemented on your platform.
          If it is unavailable, using it will raise a NotImplementedError.
        """

def makedirs(name: StrOrBytesPath, mode: int = 0o777, exist_ok: bool = False) -> None:
    """makedirs(name [, mode=0o777][, exist_ok=False])

    Super-mkdir; create a leaf directory and all intermediate ones.  Works like
    mkdir, except that any intermediate path segment (not just the rightmost)
    will be created if it does not exist. If the target directory already
    exists, raise an OSError if exist_ok is False. Otherwise no exception is
    raised.  This is recursive.

    """

if sys.platform != "win32":
    def mknod(path: StrOrBytesPath, mode: int = 0o600, device: int = 0, *, dir_fd: int | None = None) -> None:
        """Create a node in the file system.

        Create a node in the file system (file, device special file or named pipe)
        at path.  mode specifies both the permissions to use and the
        type of node to be created, being combined (bitwise OR) with one of
        S_IFREG, S_IFCHR, S_IFBLK, and S_IFIFO.  If S_IFCHR or S_IFBLK is set on mode,
        device defines the newly created device special file (probably using
        os.makedev()).  Otherwise device is ignored.

        If dir_fd is not None, it should be a file descriptor open to a directory,
          and path should be relative; path will then be relative to that directory.
        dir_fd may not be implemented on your platform.
          If it is unavailable, using it will raise a NotImplementedError.
        """

    def major(device: int, /) -> int:
        """Extracts a device major number from a raw device number."""

    def minor(device: int, /) -> int:
        """Extracts a device minor number from a raw device number."""

    def makedev(major: int, minor: int, /) -> int:
        """Composes a raw device number from the major and minor device numbers."""

    def pathconf(path: FileDescriptorOrPath, name: str | int) -> int:  # Unix only
        """Return the configuration limit name for the file or directory path.

        If there is no limit, return -1.
        On some platforms, path may also be specified as an open file descriptor.
          If this functionality is unavailable, using it raises an exception.
        """

def readlink(path: GenericPath[AnyStr], *, dir_fd: int | None = None) -> AnyStr:
    """Return a string representing the path to which the symbolic link points.

    If dir_fd is not None, it should be a file descriptor open to a directory,
    and path should be relative; path will then be relative to that directory.

    dir_fd may not be implemented on your platform.  If it is unavailable,
    using it will raise a NotImplementedError.
    """

def remove(path: StrOrBytesPath, *, dir_fd: int | None = None) -> None:
    """Remove a file (same as unlink()).

    If dir_fd is not None, it should be a file descriptor open to a directory,
      and path should be relative; path will then be relative to that directory.
    dir_fd may not be implemented on your platform.
      If it is unavailable, using it will raise a NotImplementedError.
    """

def removedirs(name: StrOrBytesPath) -> None:
    """removedirs(name)

    Super-rmdir; remove a leaf directory and all empty intermediate
    ones.  Works like rmdir except that, if the leaf directory is
    successfully removed, directories corresponding to rightmost path
    segments will be pruned away until either the whole path is
    consumed or an error occurs.  Errors during this latter phase are
    ignored -- they generally mean that a directory was not empty.

    """

def rename(src: StrOrBytesPath, dst: StrOrBytesPath, *, src_dir_fd: int | None = None, dst_dir_fd: int | None = None) -> None:
    """Rename a file or directory.

    If either src_dir_fd or dst_dir_fd is not None, it should be a file
      descriptor open to a directory, and the respective path string (src or dst)
      should be relative; the path will then be relative to that directory.
    src_dir_fd and dst_dir_fd, may not be implemented on your platform.
      If they are unavailable, using them will raise a NotImplementedError.
    """

def renames(old: StrOrBytesPath, new: StrOrBytesPath) -> None:
    """renames(old, new)

    Super-rename; create directories as necessary and delete any left
    empty.  Works like rename, except creation of any intermediate
    directories needed to make the new pathname good is attempted
    first.  After the rename, directories corresponding to rightmost
    path segments of the old name will be pruned until either the
    whole path is consumed or a nonempty directory is found.

    Note: this function can fail with the new directory structure made
    if you lack permissions needed to unlink the leaf directory or
    file.

    """

def replace(src: StrOrBytesPath, dst: StrOrBytesPath, *, src_dir_fd: int | None = None, dst_dir_fd: int | None = None) -> None:
    """Rename a file or directory, overwriting the destination.

    If either src_dir_fd or dst_dir_fd is not None, it should be a file
      descriptor open to a directory, and the respective path string (src or dst)
      should be relative; the path will then be relative to that directory.
    src_dir_fd and dst_dir_fd, may not be implemented on your platform.
      If they are unavailable, using them will raise a NotImplementedError.
    """

def rmdir(path: StrOrBytesPath, *, dir_fd: int | None = None) -> None:
    """Remove a directory.

    If dir_fd is not None, it should be a file descriptor open to a directory,
      and path should be relative; path will then be relative to that directory.
    dir_fd may not be implemented on your platform.
      If it is unavailable, using it will raise a NotImplementedError.
    """

@final
@type_check_only
class _ScandirIterator(Generic[AnyStr]):
    def __del__(self) -> None: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> DirEntry[AnyStr]: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, *args: Unused) -> None: ...
    def close(self) -> None: ...

@overload
def scandir(path: None = None) -> _ScandirIterator[str]:
    """Return an iterator of DirEntry objects for given path.

    path can be specified as either str, bytes, or a path-like object.  If path
    is bytes, the names of yielded DirEntry objects will also be bytes; in
    all other circumstances they will be str.

    If path is None, uses the path='.'.
    """

@overload
def scandir(path: int) -> _ScandirIterator[str]: ...
@overload
def scandir(path: GenericPath[AnyStr]) -> _ScandirIterator[AnyStr]: ...
def stat(path: FileDescriptorOrPath, *, dir_fd: int | None = None, follow_symlinks: bool = True) -> stat_result:
    """Perform a stat system call on the given path.

      path
        Path to be examined; can be string, bytes, a path-like object or
        open-file-descriptor int.
      dir_fd
        If not None, it should be a file descriptor open to a directory,
        and path should be a relative string; path will then be relative to
        that directory.
      follow_symlinks
        If False, and the last element of the path is a symbolic link,
        stat will examine the symbolic link itself instead of the file
        the link points to.

    dir_fd and follow_symlinks may not be implemented
      on your platform.  If they are unavailable, using them will raise a
      NotImplementedError.

    It's an error to use dir_fd or follow_symlinks when specifying path as
      an open file descriptor.
    """

if sys.platform != "win32":
    def statvfs(path: FileDescriptorOrPath) -> statvfs_result:  # Unix only
        """Perform a statvfs system call on the given path.

        path may always be specified as a string.
        On some platforms, path may also be specified as an open file descriptor.
          If this functionality is unavailable, using it raises an exception.
        """

def symlink(src: StrOrBytesPath, dst: StrOrBytesPath, target_is_directory: bool = False, *, dir_fd: int | None = None) -> None:
    """Create a symbolic link pointing to src named dst.

    target_is_directory is required on Windows if the target is to be
      interpreted as a directory.  (On Windows, symlink requires
      Windows 6.0 or greater, and raises a NotImplementedError otherwise.)
      target_is_directory is ignored on non-Windows platforms.

    If dir_fd is not None, it should be a file descriptor open to a directory,
      and path should be relative; path will then be relative to that directory.
    dir_fd may not be implemented on your platform.
      If it is unavailable, using it will raise a NotImplementedError.
    """

if sys.platform != "win32":
    def sync() -> None:  # Unix only
        """Force write of everything to disk."""

def truncate(path: FileDescriptorOrPath, length: int) -> None:  # Unix only up to version 3.4
    """Truncate a file, specified by path, to a specific length.

    On some platforms, path may also be specified as an open file descriptor.
      If this functionality is unavailable, using it raises an exception.
    """

def unlink(path: StrOrBytesPath, *, dir_fd: int | None = None) -> None:
    """Remove a file (same as remove()).

    If dir_fd is not None, it should be a file descriptor open to a directory,
      and path should be relative; path will then be relative to that directory.
    dir_fd may not be implemented on your platform.
      If it is unavailable, using it will raise a NotImplementedError.
    """

def utime(
    path: FileDescriptorOrPath,
    times: tuple[int, int] | tuple[float, float] | None = None,
    *,
    ns: tuple[int, int] = ...,
    dir_fd: int | None = None,
    follow_symlinks: bool = True,
) -> None:
    """Set the access and modified time of path.

    path may always be specified as a string.
    On some platforms, path may also be specified as an open file descriptor.
      If this functionality is unavailable, using it raises an exception.

    If times is not None, it must be a tuple (atime, mtime);
        atime and mtime should be expressed as float seconds since the epoch.
    If ns is specified, it must be a tuple (atime_ns, mtime_ns);
        atime_ns and mtime_ns should be expressed as integer nanoseconds
        since the epoch.
    If times is None and ns is unspecified, utime uses the current time.
    Specifying tuples for both times and ns is an error.

    If dir_fd is not None, it should be a file descriptor open to a directory,
      and path should be relative; path will then be relative to that directory.
    If follow_symlinks is False, and the last element of the path is a symbolic
      link, utime will modify the symbolic link itself instead of the file the
      link points to.
    It is an error to use dir_fd or follow_symlinks when specifying path
      as an open file descriptor.
    dir_fd and follow_symlinks may not be available on your platform.
      If they are unavailable, using them will raise a NotImplementedError.
    """

_OnError: TypeAlias = Callable[[OSError], object]

def walk(
    top: GenericPath[AnyStr], topdown: bool = True, onerror: _OnError | None = None, followlinks: bool = False
) -> Iterator[tuple[AnyStr, list[AnyStr], list[AnyStr]]]:
    """Directory tree generator.

    For each directory in the directory tree rooted at top (including top
    itself, but excluding '.' and '..'), yields a 3-tuple

        dirpath, dirnames, filenames

    dirpath is a string, the path to the directory.  dirnames is a list of
    the names of the subdirectories in dirpath (including symlinks to directories,
    and excluding '.' and '..').
    filenames is a list of the names of the non-directory files in dirpath.
    Note that the names in the lists are just names, with no path components.
    To get a full path (which begins with top) to a file or directory in
    dirpath, do os.path.join(dirpath, name).

    If optional arg 'topdown' is true or not specified, the triple for a
    directory is generated before the triples for any of its subdirectories
    (directories are generated top down).  If topdown is false, the triple
    for a directory is generated after the triples for all of its
    subdirectories (directories are generated bottom up).

    When topdown is true, the caller can modify the dirnames list in-place
    (e.g., via del or slice assignment), and walk will only recurse into the
    subdirectories whose names remain in dirnames; this can be used to prune the
    search, or to impose a specific order of visiting.  Modifying dirnames when
    topdown is false has no effect on the behavior of os.walk(), since the
    directories in dirnames have already been generated by the time dirnames
    itself is generated. No matter the value of topdown, the list of
    subdirectories is retrieved before the tuples for the directory and its
    subdirectories are generated.

    By default errors from the os.scandir() call are ignored.  If
    optional arg 'onerror' is specified, it should be a function; it
    will be called with one argument, an OSError instance.  It can
    report the error to continue with the walk, or raise the exception
    to abort the walk.  Note that the filename is available as the
    filename attribute of the exception object.

    By default, os.walk does not follow symbolic links to subdirectories on
    systems that support them.  In order to get this functionality, set the
    optional argument 'followlinks' to true.

    Caution:  if you pass a relative pathname for top, don't change the
    current working directory between resumptions of walk.  walk never
    changes the current directory, and assumes that the client doesn't
    either.

    Example:

    import os
    from os.path import join, getsize
    for root, dirs, files in os.walk('python/Lib/xml'):
        print(root, "consumes ")
        print(sum(getsize(join(root, name)) for name in files), end=" ")
        print("bytes in", len(files), "non-directory files")
        if '__pycache__' in dirs:
            dirs.remove('__pycache__')  # don't visit __pycache__ directories

    """

if sys.platform != "win32":
    @overload
    def fwalk(
        top: StrPath = ".",
        topdown: bool = True,
        onerror: _OnError | None = None,
        *,
        follow_symlinks: bool = False,
        dir_fd: int | None = None,
    ) -> Iterator[tuple[str, list[str], list[str], int]]:
        """Directory tree generator.

        This behaves exactly like walk(), except that it yields a 4-tuple

            dirpath, dirnames, filenames, dirfd

        `dirpath`, `dirnames` and `filenames` are identical to walk() output,
        and `dirfd` is a file descriptor referring to the directory `dirpath`.

        The advantage of fwalk() over walk() is that it's safe against symlink
        races (when follow_symlinks is False).

        If dir_fd is not None, it should be a file descriptor open to a directory,
          and top should be relative; top will then be relative to that directory.
          (dir_fd is always supported for fwalk.)

        Caution:
        Since fwalk() yields file descriptors, those are only valid until the
        next iteration step, so you should dup() them if you want to keep them
        for a longer period.

        Example:

        import os
        for root, dirs, files, rootfd in os.fwalk('python/Lib/xml'):
            print(root, "consumes", end="")
            print(sum(os.stat(name, dir_fd=rootfd).st_size for name in files),
                  end="")
            print("bytes in", len(files), "non-directory files")
            if '__pycache__' in dirs:
                dirs.remove('__pycache__')  # don't visit __pycache__ directories
        """

    @overload
    def fwalk(
        top: BytesPath,
        topdown: bool = True,
        onerror: _OnError | None = None,
        *,
        follow_symlinks: bool = False,
        dir_fd: int | None = None,
    ) -> Iterator[tuple[bytes, list[bytes], list[bytes], int]]: ...
    if sys.platform == "linux":
        def getxattr(path: FileDescriptorOrPath, attribute: StrOrBytesPath, *, follow_symlinks: bool = True) -> bytes:
            """Return the value of extended attribute attribute on path.

            path may be either a string, a path-like object, or an open file descriptor.
            If follow_symlinks is False, and the last element of the path is a symbolic
              link, getxattr will examine the symbolic link itself instead of the file
              the link points to.
            """

        def listxattr(path: FileDescriptorOrPath | None = None, *, follow_symlinks: bool = True) -> list[str]:
            """Return a list of extended attributes on path.

            path may be either None, a string, a path-like object, or an open file descriptor.
            if path is None, listxattr will examine the current directory.
            If follow_symlinks is False, and the last element of the path is a symbolic
              link, listxattr will examine the symbolic link itself instead of the file
              the link points to.
            """

        def removexattr(path: FileDescriptorOrPath, attribute: StrOrBytesPath, *, follow_symlinks: bool = True) -> None:
            """Remove extended attribute attribute on path.

            path may be either a string, a path-like object, or an open file descriptor.
            If follow_symlinks is False, and the last element of the path is a symbolic
              link, removexattr will modify the symbolic link itself instead of the file
              the link points to.
            """

        def setxattr(
            path: FileDescriptorOrPath,
            attribute: StrOrBytesPath,
            value: ReadableBuffer,
            flags: int = 0,
            *,
            follow_symlinks: bool = True,
        ) -> None:
            """Set extended attribute attribute on path to value.

            path may be either a string, a path-like object,  or an open file descriptor.
            If follow_symlinks is False, and the last element of the path is a symbolic
              link, setxattr will modify the symbolic link itself instead of the file
              the link points to.
            """

def abort() -> NoReturn:
    """Abort the interpreter immediately.

    This function 'dumps core' or otherwise fails in the hardest way possible
    on the hosting operating system.  This function never returns.
    """

# These are defined as execl(file, *args) but the first *arg is mandatory.
def execl(file: StrOrBytesPath, *args: Unpack[tuple[StrOrBytesPath, Unpack[tuple[StrOrBytesPath, ...]]]]) -> NoReturn:
    """execl(file, *args)

    Execute the executable file with argument list args, replacing the
    current process.
    """

def execlp(file: StrOrBytesPath, *args: Unpack[tuple[StrOrBytesPath, Unpack[tuple[StrOrBytesPath, ...]]]]) -> NoReturn:
    """execlp(file, *args)

    Execute the executable file (which is searched for along $PATH)
    with argument list args, replacing the current process.
    """

# These are: execle(file, *args, env) but env is pulled from the last element of the args.
def execle(file: StrOrBytesPath, *args: Unpack[tuple[StrOrBytesPath, Unpack[tuple[StrOrBytesPath, ...]], _ExecEnv]]) -> NoReturn:
    """execle(file, *args, env)

    Execute the executable file with argument list args and
    environment env, replacing the current process.
    """

def execlpe(file: StrOrBytesPath, *args: Unpack[tuple[StrOrBytesPath, Unpack[tuple[StrOrBytesPath, ...]], _ExecEnv]]) -> NoReturn:
    """execlpe(file, *args, env)

    Execute the executable file (which is searched for along $PATH)
    with argument list args and environment env, replacing the current
    process.
    """

# The docs say `args: tuple or list of strings`
# The implementation enforces tuple or list so we can't use Sequence.
# Not separating out PathLike[str] and PathLike[bytes] here because it doesn't make much difference
# in practice, and doing so would explode the number of combinations in this already long union.
# All these combinations are necessary due to list being invariant.
_ExecVArgs: TypeAlias = (
    tuple[StrOrBytesPath, ...]
    | list[bytes]
    | list[str]
    | list[PathLike[Any]]
    | list[bytes | str]
    | list[bytes | PathLike[Any]]
    | list[str | PathLike[Any]]
    | list[bytes | str | PathLike[Any]]
)
# Depending on the OS, the keys and values are passed either to
# PyUnicode_FSDecoder (which accepts str | ReadableBuffer) or to
# PyUnicode_FSConverter (which accepts StrOrBytesPath). For simplicity,
# we limit to str | bytes.
_ExecEnv: TypeAlias = Mapping[bytes, bytes | str] | Mapping[str, bytes | str]

def execv(path: StrOrBytesPath, argv: _ExecVArgs, /) -> NoReturn:
    """Execute an executable path with arguments, replacing current process.

    path
      Path of executable file.
    argv
      Tuple or list of strings.
    """

def execve(path: FileDescriptorOrPath, argv: _ExecVArgs, env: _ExecEnv) -> NoReturn:
    """Execute an executable path with arguments, replacing current process.

    path
      Path of executable file.
    argv
      Tuple or list of strings.
    env
      Dictionary of strings mapping to strings.
    """

def execvp(file: StrOrBytesPath, args: _ExecVArgs) -> NoReturn:
    """execvp(file, args)

    Execute the executable file (which is searched for along $PATH)
    with argument list args, replacing the current process.
    args may be a list or tuple of strings.
    """

def execvpe(file: StrOrBytesPath, args: _ExecVArgs, env: _ExecEnv) -> NoReturn:
    """execvpe(file, args, env)

    Execute the executable file (which is searched for along $PATH)
    with argument list args and environment env, replacing the
    current process.
    args may be a list or tuple of strings.
    """

def _exit(status: int) -> NoReturn:
    """Exit to the system with specified status, without normal exit processing."""

def kill(pid: int, signal: int, /) -> None:
    """Kill a process with a signal."""

if sys.platform != "win32":
    # Unix only
    def fork() -> int:
        """Fork a child process.

        Return 0 to child process and PID of child to parent process.
        """

    def forkpty() -> tuple[int, int]:  # some flavors of Unix
        """Fork a new process with a new pseudo-terminal as controlling tty.

        Returns a tuple of (pid, master_fd).
        Like fork(), return pid of 0 to the child process,
        and pid of child to the parent process.
        To both, return fd of newly opened pseudo-terminal.
        """

    def killpg(pgid: int, signal: int, /) -> None:
        """Kill a process group with a signal."""

    def nice(increment: int, /) -> int:
        """Add increment to the priority of process and return the new priority."""
    if sys.platform != "darwin" and sys.platform != "linux":
        def plock(op: int, /) -> None: ...

class _wrap_close:
    def __init__(self, stream: TextIOWrapper, proc: Popen[str]) -> None: ...
    def close(self) -> int | None: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
    ) -> None: ...
    def __iter__(self) -> Iterator[str]: ...
    # Methods below here don't exist directly on the _wrap_close object, but
    # are copied from the wrapped TextIOWrapper object via __getattr__.
    # The full set of TextIOWrapper methods are technically available this way,
    # but undocumented. Only a subset are currently included here.
    def read(self, size: int | None = -1, /) -> str: ...
    def readable(self) -> bool: ...
    def readline(self, size: int = -1, /) -> str: ...
    def readlines(self, hint: int = -1, /) -> list[str]: ...
    def writable(self) -> bool: ...
    def write(self, s: str, /) -> int: ...
    def writelines(self, lines: Iterable[str], /) -> None: ...

def popen(cmd: str, mode: str = "r", buffering: int = -1) -> _wrap_close: ...
def spawnl(mode: int, file: StrOrBytesPath, arg0: StrOrBytesPath, *args: StrOrBytesPath) -> int:
    """spawnl(mode, file, *args) -> integer

    Execute file with arguments from args in a subprocess.
    If mode == P_NOWAIT return the pid of the process.
    If mode == P_WAIT return the process's exit code if it exits normally;
    otherwise return -SIG, where SIG is the signal that killed it.
    """

def spawnle(mode: int, file: StrOrBytesPath, arg0: StrOrBytesPath, *args: Any) -> int:  # Imprecise sig
    """spawnle(mode, file, *args, env) -> integer

    Execute file with arguments from args in a subprocess with the
    supplied environment.
    If mode == P_NOWAIT return the pid of the process.
    If mode == P_WAIT return the process's exit code if it exits normally;
    otherwise return -SIG, where SIG is the signal that killed it.
    """

if sys.platform != "win32":
    def spawnv(mode: int, file: StrOrBytesPath, args: _ExecVArgs) -> int:
        """spawnv(mode, file, args) -> integer

        Execute file with arguments from args in a subprocess.
        If mode == P_NOWAIT return the pid of the process.
        If mode == P_WAIT return the process's exit code if it exits normally;
        otherwise return -SIG, where SIG is the signal that killed it.
        """

    def spawnve(mode: int, file: StrOrBytesPath, args: _ExecVArgs, env: _ExecEnv) -> int:
        """spawnve(mode, file, args, env) -> integer

        Execute file with arguments from args in a subprocess with the
        specified environment.
        If mode == P_NOWAIT return the pid of the process.
        If mode == P_WAIT return the process's exit code if it exits normally;
        otherwise return -SIG, where SIG is the signal that killed it.
        """

else:
    def spawnv(mode: int, path: StrOrBytesPath, argv: _ExecVArgs, /) -> int:
        """Execute the program specified by path in a new process.

        mode
          Mode of process creation.
        path
          Path of executable file.
        argv
          Tuple or list of strings.
        """

    def spawnve(mode: int, path: StrOrBytesPath, argv: _ExecVArgs, env: _ExecEnv, /) -> int:
        """Execute the program specified by path in a new process.

        mode
          Mode of process creation.
        path
          Path of executable file.
        argv
          Tuple or list of strings.
        env
          Dictionary of strings mapping to strings.
        """

def system(command: StrOrBytesPath) -> int:
    """Execute the command in a subshell."""

@final
class times_result(structseq[float], tuple[float, float, float, float, float]):
    """times_result: Result from os.times().

    This object may be accessed either as a tuple of
      (user, system, children_user, children_system, elapsed),
    or via the attributes user, system, children_user, children_system,
    and elapsed.

    See os.times for more information.
    """

    if sys.version_info >= (3, 10):
        __match_args__: Final = ("user", "system", "children_user", "children_system", "elapsed")

    @property
    def user(self) -> float:
        """user time"""

    @property
    def system(self) -> float:
        """system time"""

    @property
    def children_user(self) -> float:
        """user time of children"""

    @property
    def children_system(self) -> float:
        """system time of children"""

    @property
    def elapsed(self) -> float:
        """elapsed time since an arbitrary point in the past"""

def times() -> times_result:
    """Return a collection containing process timing information.

    The object returned behaves like a named tuple with these fields:
      (utime, stime, cutime, cstime, elapsed_time)
    All fields are floating-point numbers.
    """

def waitpid(pid: int, options: int, /) -> tuple[int, int]:
    """Wait for completion of a given child process.

    Returns a tuple of information regarding the child process:
        (pid, status)

    The options argument is ignored on Windows.
    """

if sys.platform == "win32":
    if sys.version_info >= (3, 10):
        def startfile(
            filepath: StrOrBytesPath,
            operation: str = ...,
            arguments: str = "",
            cwd: StrOrBytesPath | None = None,
            show_cmd: int = 1,
        ) -> None:
            """Start a file with its associated application.

            When "operation" is not specified or "open", this acts like
            double-clicking the file in Explorer, or giving the file name as an
            argument to the DOS "start" command: the file is opened with whatever
            application (if any) its extension is associated.
            When another "operation" is given, it specifies what should be done with
            the file.  A typical operation is "print".

            "arguments" is passed to the application, but should be omitted if the
            file is a document.

            "cwd" is the working directory for the operation. If "filepath" is
            relative, it will be resolved against this directory. This argument
            should usually be an absolute path.

            "show_cmd" can be used to override the recommended visibility option.
            See the Windows ShellExecute documentation for values.

            startfile returns as soon as the associated application is launched.
            There is no option to wait for the application to close, and no way
            to retrieve the application's exit status.

            The filepath is relative to the current directory.  If you want to use
            an absolute path, make sure the first character is not a slash ("/");
            the underlying Win32 ShellExecute function doesn't work if it is.
            """
    else:
        def startfile(filepath: StrOrBytesPath, operation: str = ...) -> None:
            """Start a file with its associated application.

            When "operation" is not specified or "open", this acts like
            double-clicking the file in Explorer, or giving the file name as an
            argument to the DOS "start" command: the file is opened with whatever
            application (if any) its extension is associated.
            When another "operation" is given, it specifies what should be done with
            the file.  A typical operation is "print".

            startfile returns as soon as the associated application is launched.
            There is no option to wait for the application to close, and no way
            to retrieve the application's exit status.

            The filepath is relative to the current directory.  If you want to use
            an absolute path, make sure the first character is not a slash ("/");
            the underlying Win32 ShellExecute function doesn't work if it is.
            """

else:
    def spawnlp(mode: int, file: StrOrBytesPath, arg0: StrOrBytesPath, *args: StrOrBytesPath) -> int:
        """spawnlp(mode, file, *args) -> integer

        Execute file (which is looked for along $PATH) with arguments from
        args in a subprocess with the supplied environment.
        If mode == P_NOWAIT return the pid of the process.
        If mode == P_WAIT return the process's exit code if it exits normally;
        otherwise return -SIG, where SIG is the signal that killed it.
        """

    def spawnlpe(mode: int, file: StrOrBytesPath, arg0: StrOrBytesPath, *args: Any) -> int:  # Imprecise signature
        """spawnlpe(mode, file, *args, env) -> integer

        Execute file (which is looked for along $PATH) with arguments from
        args in a subprocess with the supplied environment.
        If mode == P_NOWAIT return the pid of the process.
        If mode == P_WAIT return the process's exit code if it exits normally;
        otherwise return -SIG, where SIG is the signal that killed it.
        """

    def spawnvp(mode: int, file: StrOrBytesPath, args: _ExecVArgs) -> int:
        """spawnvp(mode, file, args) -> integer

        Execute file (which is looked for along $PATH) with arguments from
        args in a subprocess.
        If mode == P_NOWAIT return the pid of the process.
        If mode == P_WAIT return the process's exit code if it exits normally;
        otherwise return -SIG, where SIG is the signal that killed it.
        """

    def spawnvpe(mode: int, file: StrOrBytesPath, args: _ExecVArgs, env: _ExecEnv) -> int:
        """spawnvpe(mode, file, args, env) -> integer

        Execute file (which is looked for along $PATH) with arguments from
        args in a subprocess with the supplied environment.
        If mode == P_NOWAIT return the pid of the process.
        If mode == P_WAIT return the process's exit code if it exits normally;
        otherwise return -SIG, where SIG is the signal that killed it.
        """

    def wait() -> tuple[int, int]:  # Unix only
        """Wait for completion of a child process.

        Returns a tuple of information about the child process:
            (pid, status)
        """
    # Added to MacOS in 3.13
    if sys.platform != "darwin" or sys.version_info >= (3, 13):
        @final
        class waitid_result(structseq[int], tuple[int, int, int, int, int]):
            """waitid_result: Result from waitid.

            This object may be accessed either as a tuple of
              (si_pid, si_uid, si_signo, si_status, si_code),
            or via the attributes si_pid, si_uid, and so on.

            See os.waitid for more information.
            """

            if sys.version_info >= (3, 10):
                __match_args__: Final = ("si_pid", "si_uid", "si_signo", "si_status", "si_code")

            @property
            def si_pid(self) -> int: ...
            @property
            def si_uid(self) -> int: ...
            @property
            def si_signo(self) -> int: ...
            @property
            def si_status(self) -> int: ...
            @property
            def si_code(self) -> int: ...

        def waitid(idtype: int, ident: int, options: int, /) -> waitid_result | None:
            """Returns the result of waiting for a process or processes.

              idtype
                Must be one of be P_PID, P_PGID or P_ALL.
              id
                The id to wait on.
              options
                Constructed from the ORing of one or more of WEXITED, WSTOPPED
                or WCONTINUED and additionally may be ORed with WNOHANG or WNOWAIT.

            Returns either waitid_result or None if WNOHANG is specified and there are
            no children in a waitable state.
            """
    from resource import struct_rusage

    def wait3(options: int) -> tuple[int, int, struct_rusage]:
        """Wait for completion of a child process.

        Returns a tuple of information about the child process:
          (pid, status, rusage)
        """

    def wait4(pid: int, options: int) -> tuple[int, int, struct_rusage]:
        """Wait for completion of a specific child process.

        Returns a tuple of information about the child process:
          (pid, status, rusage)
        """

    def WCOREDUMP(status: int, /) -> bool:
        """Return True if the process returning status was dumped to a core file."""

    def WIFCONTINUED(status: int) -> bool:
        """Return True if a particular process was continued from a job control stop.

        Return True if the process returning status was continued from a
        job control stop.
        """

    def WIFSTOPPED(status: int) -> bool:
        """Return True if the process returning status was stopped."""

    def WIFSIGNALED(status: int) -> bool:
        """Return True if the process returning status was terminated by a signal."""

    def WIFEXITED(status: int) -> bool:
        """Return True if the process returning status exited via the exit() system call."""

    def WEXITSTATUS(status: int) -> int:
        """Return the process return code from status."""

    def WSTOPSIG(status: int) -> int:
        """Return the signal that stopped the process that provided the status value."""

    def WTERMSIG(status: int) -> int:
        """Return the signal that terminated the process that provided the status value."""
    if sys.version_info >= (3, 13):
        def posix_spawn(
            path: StrOrBytesPath,
            argv: _ExecVArgs,
            env: _ExecEnv | None,  # None allowed starting in 3.13
            /,
            *,
            file_actions: Sequence[tuple[Any, ...]] | None = ...,
            setpgroup: int | None = ...,
            resetids: bool = ...,
            setsid: bool = ...,
            setsigmask: Iterable[int] = ...,
            setsigdef: Iterable[int] = ...,
            scheduler: tuple[Any, sched_param] | None = ...,
        ) -> int:
            """Execute the program specified by path in a new process.

            path
              Path of executable file.
            argv
              Tuple or list of strings.
            env
              Dictionary of strings mapping to strings.
            file_actions
              A sequence of file action tuples.
            setpgroup
              The pgroup to use with the POSIX_SPAWN_SETPGROUP flag.
            resetids
              If the value is `true` the POSIX_SPAWN_RESETIDS will be activated.
            setsid
              If the value is `true` the POSIX_SPAWN_SETSID or POSIX_SPAWN_SETSID_NP will be activated.
            setsigmask
              The sigmask to use with the POSIX_SPAWN_SETSIGMASK flag.
            setsigdef
              The sigmask to use with the POSIX_SPAWN_SETSIGDEF flag.
            scheduler
              A tuple with the scheduler policy (optional) and parameters.
            """

        def posix_spawnp(
            path: StrOrBytesPath,
            argv: _ExecVArgs,
            env: _ExecEnv | None,  # None allowed starting in 3.13
            /,
            *,
            file_actions: Sequence[tuple[Any, ...]] | None = ...,
            setpgroup: int | None = ...,
            resetids: bool = ...,
            setsid: bool = ...,
            setsigmask: Iterable[int] = ...,
            setsigdef: Iterable[int] = ...,
            scheduler: tuple[Any, sched_param] | None = ...,
        ) -> int:
            """Execute the program specified by path in a new process.

            path
              Path of executable file.
            argv
              Tuple or list of strings.
            env
              Dictionary of strings mapping to strings.
            file_actions
              A sequence of file action tuples.
            setpgroup
              The pgroup to use with the POSIX_SPAWN_SETPGROUP flag.
            resetids
              If the value is `True` the POSIX_SPAWN_RESETIDS will be activated.
            setsid
              If the value is `True` the POSIX_SPAWN_SETSID or POSIX_SPAWN_SETSID_NP will be activated.
            setsigmask
              The sigmask to use with the POSIX_SPAWN_SETSIGMASK flag.
            setsigdef
              The sigmask to use with the POSIX_SPAWN_SETSIGDEF flag.
            scheduler
              A tuple with the scheduler policy (optional) and parameters.
            """
    else:
        def posix_spawn(
            path: StrOrBytesPath,
            argv: _ExecVArgs,
            env: _ExecEnv,
            /,
            *,
            file_actions: Sequence[tuple[Any, ...]] | None = ...,
            setpgroup: int | None = ...,
            resetids: bool = ...,
            setsid: bool = ...,
            setsigmask: Iterable[int] = ...,
            setsigdef: Iterable[int] = ...,
            scheduler: tuple[Any, sched_param] | None = ...,
        ) -> int:
            """Execute the program specified by path in a new process.

            path
              Path of executable file.
            argv
              Tuple or list of strings.
            env
              Dictionary of strings mapping to strings.
            file_actions
              A sequence of file action tuples.
            setpgroup
              The pgroup to use with the POSIX_SPAWN_SETPGROUP flag.
            resetids
              If the value is `true` the POSIX_SPAWN_RESETIDS will be activated.
            setsid
              If the value is `true` the POSIX_SPAWN_SETSID or POSIX_SPAWN_SETSID_NP will be activated.
            setsigmask
              The sigmask to use with the POSIX_SPAWN_SETSIGMASK flag.
            setsigdef
              The sigmask to use with the POSIX_SPAWN_SETSIGDEF flag.
            scheduler
              A tuple with the scheduler policy (optional) and parameters.
            """

        def posix_spawnp(
            path: StrOrBytesPath,
            argv: _ExecVArgs,
            env: _ExecEnv,
            /,
            *,
            file_actions: Sequence[tuple[Any, ...]] | None = ...,
            setpgroup: int | None = ...,
            resetids: bool = ...,
            setsid: bool = ...,
            setsigmask: Iterable[int] = ...,
            setsigdef: Iterable[int] = ...,
            scheduler: tuple[Any, sched_param] | None = ...,
        ) -> int:
            """Execute the program specified by path in a new process.

            path
              Path of executable file.
            argv
              Tuple or list of strings.
            env
              Dictionary of strings mapping to strings.
            file_actions
              A sequence of file action tuples.
            setpgroup
              The pgroup to use with the POSIX_SPAWN_SETPGROUP flag.
            resetids
              If the value is `True` the POSIX_SPAWN_RESETIDS will be activated.
            setsid
              If the value is `True` the POSIX_SPAWN_SETSID or POSIX_SPAWN_SETSID_NP will be activated.
            setsigmask
              The sigmask to use with the POSIX_SPAWN_SETSIGMASK flag.
            setsigdef
              The sigmask to use with the POSIX_SPAWN_SETSIGDEF flag.
            scheduler
              A tuple with the scheduler policy (optional) and parameters.
            """
    POSIX_SPAWN_OPEN: Final = 0
    POSIX_SPAWN_CLOSE: Final = 1
    POSIX_SPAWN_DUP2: Final = 2

if sys.platform != "win32":
    @final
    class sched_param(structseq[int], tuple[int]):
        """Currently has only one field: sched_priority

        sched_priority
          A scheduling parameter.
        """

        if sys.version_info >= (3, 10):
            __match_args__: Final = ("sched_priority",)

        def __new__(cls, sched_priority: int) -> Self: ...
        @property
        def sched_priority(self) -> int:
            """the scheduling priority"""

    def sched_get_priority_min(policy: int) -> int:  # some flavors of Unix
        """Get the minimum scheduling priority for policy."""

    def sched_get_priority_max(policy: int) -> int:  # some flavors of Unix
        """Get the maximum scheduling priority for policy."""

    def sched_yield() -> None:  # some flavors of Unix
        """Voluntarily relinquish the CPU."""
    if sys.platform != "darwin":
        def sched_setscheduler(pid: int, policy: int, param: sched_param, /) -> None:  # some flavors of Unix
            """Set the scheduling policy for the process identified by pid.

            If pid is 0, the calling process is changed.
            param is an instance of sched_param.
            """

        def sched_getscheduler(pid: int, /) -> int:  # some flavors of Unix
            """Get the scheduling policy for the process identified by pid.

            Passing 0 for pid returns the scheduling policy for the calling process.
            """

        def sched_rr_get_interval(pid: int, /) -> float:  # some flavors of Unix
            """Return the round-robin quantum for the process identified by pid, in seconds.

            Value returned is a float.
            """

        def sched_setparam(pid: int, param: sched_param, /) -> None:  # some flavors of Unix
            """Set scheduling parameters for the process identified by pid.

            If pid is 0, sets parameters for the calling process.
            param should be an instance of sched_param.
            """

        def sched_getparam(pid: int, /) -> sched_param:  # some flavors of Unix
            """Returns scheduling parameters for the process identified by pid.

            If pid is 0, returns parameters for the calling process.
            Return value is an instance of sched_param.
            """

        def sched_setaffinity(pid: int, mask: Iterable[int], /) -> None:  # some flavors of Unix
            """Set the CPU affinity of the process identified by pid to mask.

            mask should be an iterable of integers identifying CPUs.
            """

        def sched_getaffinity(pid: int, /) -> set[int]:  # some flavors of Unix
            """Return the affinity of the process identified by pid (or the current process if zero).

            The affinity is returned as a set of CPU identifiers.
            """

def cpu_count() -> int | None:
    """Return the number of logical CPUs in the system.

    Return None if indeterminable.
    """

if sys.version_info >= (3, 13):
    # Documented to return `int | None`, but falls back to `len(sched_getaffinity(0))` when
    # available. See https://github.com/python/cpython/blob/417c130/Lib/os.py#L1175-L1186.
    if sys.platform != "win32" and sys.platform != "darwin":
        def process_cpu_count() -> int:
            """
            Get the number of CPUs of the current process.

            Return the number of logical CPUs usable by the calling thread of the
            current process. Return None if indeterminable.
            """
    else:
        def process_cpu_count() -> int | None:
            """Return the number of logical CPUs in the system.

            Return None if indeterminable.
            """

if sys.platform != "win32":
    # Unix only
    def confstr(name: str | int, /) -> str | None:
        """Return a string-valued system configuration variable."""

    def getloadavg() -> tuple[float, float, float]:
        """Return average recent system load information.

        Return the number of processes in the system run queue averaged over
        the last 1, 5, and 15 minutes as a tuple of three floats.
        Raises OSError if the load average was unobtainable.
        """

    def sysconf(name: str | int, /) -> int:
        """Return an integer-valued system configuration variable."""

if sys.platform == "linux":
    def getrandom(size: int, flags: int = 0) -> bytes:
        """Obtain a series of random bytes."""

def urandom(size: int, /) -> bytes:
    """Return a bytes object containing random bytes suitable for cryptographic use."""

if sys.platform != "win32":
    def register_at_fork(
        *,
        before: Callable[..., Any] | None = ...,
        after_in_parent: Callable[..., Any] | None = ...,
        after_in_child: Callable[..., Any] | None = ...,
    ) -> None:
        """Register callables to be called when forking a new process.

          before
            A callable to be called in the parent before the fork() syscall.
          after_in_child
            A callable to be called in the child after fork().
          after_in_parent
            A callable to be called in the parent after fork().

        'before' callbacks are called in reverse order.
        'after_in_child' and 'after_in_parent' callbacks are called in order.
        """

if sys.platform == "win32":
    class _AddedDllDirectory:
        path: str | None
        def __init__(self, path: str | None, cookie: _T, remove_dll_directory: Callable[[_T], object]) -> None: ...
        def close(self) -> None: ...
        def __enter__(self) -> Self: ...
        def __exit__(self, *args: Unused) -> None: ...

    def add_dll_directory(path: str) -> _AddedDllDirectory:
        """Add a path to the DLL search path.

        This search path is used when resolving dependencies for imported
        extension modules (the module itself is resolved through sys.path),
        and also by ctypes.

        Remove the directory by calling close() on the returned object or
        using it in a with statement.
        """

if sys.platform == "linux":
    MFD_CLOEXEC: Final[int]
    MFD_ALLOW_SEALING: Final[int]
    MFD_HUGETLB: Final[int]
    MFD_HUGE_SHIFT: Final[int]
    MFD_HUGE_MASK: Final[int]
    MFD_HUGE_64KB: Final[int]
    MFD_HUGE_512KB: Final[int]
    MFD_HUGE_1MB: Final[int]
    MFD_HUGE_2MB: Final[int]
    MFD_HUGE_8MB: Final[int]
    MFD_HUGE_16MB: Final[int]
    MFD_HUGE_32MB: Final[int]
    MFD_HUGE_256MB: Final[int]
    MFD_HUGE_512MB: Final[int]
    MFD_HUGE_1GB: Final[int]
    MFD_HUGE_2GB: Final[int]
    MFD_HUGE_16GB: Final[int]
    def memfd_create(name: str, flags: int = ...) -> int: ...
    def copy_file_range(src: int, dst: int, count: int, offset_src: int | None = ..., offset_dst: int | None = ...) -> int:
        """Copy count bytes from one file descriptor to another.

          src
            Source file descriptor.
          dst
            Destination file descriptor.
          count
            Number of bytes to copy.
          offset_src
            Starting offset in src.
          offset_dst
            Starting offset in dst.

        If offset_src is None, then src is read from the current position;
        respectively for offset_dst.
        """

def waitstatus_to_exitcode(status: int) -> int:
    """Convert a wait status to an exit code.

    On Unix:

    * If WIFEXITED(status) is true, return WEXITSTATUS(status).
    * If WIFSIGNALED(status) is true, return -WTERMSIG(status).
    * Otherwise, raise a ValueError.

    On Windows, return status shifted right by 8 bits.

    On Unix, if the process is being traced or if waitpid() was called with
    WUNTRACED option, the caller must first check if WIFSTOPPED(status) is true.
    This function must not be called if WIFSTOPPED(status) is true.
    """

if sys.platform == "linux":
    def pidfd_open(pid: int, flags: int = ...) -> int:
        """Return a file descriptor referring to the process *pid*.

        The descriptor can be used to perform process management without races and
        signals.
        """

if sys.version_info >= (3, 12) and sys.platform == "linux":
    PIDFD_NONBLOCK: Final = 2048

if sys.version_info >= (3, 12) and sys.platform == "win32":
    def listdrives() -> list[str]:
        """Return a list containing the names of drives in the system.

        A drive name typically looks like 'C:\\\\'.
        """

    def listmounts(volume: str) -> list[str]:
        """Return a list containing mount points for a particular volume.

        'volume' should be a GUID path as returned from os.listvolumes.
        """

    def listvolumes() -> list[str]:
        """Return a list containing the volumes in the system.

        Volumes are typically represented as a GUID path.
        """

if sys.version_info >= (3, 10) and sys.platform == "linux":
    EFD_CLOEXEC: Final[int]
    EFD_NONBLOCK: Final[int]
    EFD_SEMAPHORE: Final[int]
    SPLICE_F_MORE: Final[int]
    SPLICE_F_MOVE: Final[int]
    SPLICE_F_NONBLOCK: Final[int]
    def eventfd(initval: int, flags: int = 524288) -> FileDescriptor:
        """Creates and returns an event notification file descriptor."""

    def eventfd_read(fd: FileDescriptor) -> int:
        """Read eventfd value"""

    def eventfd_write(fd: FileDescriptor, value: int) -> None:
        """Write eventfd value."""

    def splice(
        src: FileDescriptor,
        dst: FileDescriptor,
        count: int,
        offset_src: int | None = ...,
        offset_dst: int | None = ...,
        flags: int = 0,
    ) -> int:
        """Transfer count bytes from one pipe to a descriptor or vice versa.

          src
            Source file descriptor.
          dst
            Destination file descriptor.
          count
            Number of bytes to copy.
          offset_src
            Starting offset in src.
          offset_dst
            Starting offset in dst.
          flags
            Flags to modify the semantics of the call.

        If offset_src is None, then src is read from the current position;
        respectively for offset_dst. The offset associated to the file
        descriptor that refers to a pipe must be None.
        """

if sys.version_info >= (3, 12) and sys.platform == "linux":
    CLONE_FILES: Final[int]
    CLONE_FS: Final[int]
    CLONE_NEWCGROUP: Final[int]  # Linux 4.6+
    CLONE_NEWIPC: Final[int]  # Linux 2.6.19+
    CLONE_NEWNET: Final[int]  # Linux 2.6.24+
    CLONE_NEWNS: Final[int]
    CLONE_NEWPID: Final[int]  # Linux 3.8+
    CLONE_NEWTIME: Final[int]  # Linux 5.6+
    CLONE_NEWUSER: Final[int]  # Linux 3.8+
    CLONE_NEWUTS: Final[int]  # Linux 2.6.19+
    CLONE_SIGHAND: Final[int]
    CLONE_SYSVSEM: Final[int]  # Linux 2.6.26+
    CLONE_THREAD: Final[int]
    CLONE_VM: Final[int]
    def unshare(flags: int) -> None:
        """Disassociate parts of a process (or thread) execution context.

        flags
          Namespaces to be unshared.
        """

    def setns(fd: FileDescriptorLike, nstype: int = 0) -> None:
        """Move the calling thread into different namespaces.

        fd
          A file descriptor to a namespace.
        nstype
          Type of namespace.
        """

if sys.version_info >= (3, 13) and sys.platform != "win32":
    def posix_openpt(oflag: int, /) -> int:
        """Open and return a file descriptor for a master pseudo-terminal device.

        Performs a posix_openpt() C function call. The oflag argument is used to
        set file status flags and file access modes as specified in the manual page
        of posix_openpt() of your system.
        """

    def grantpt(fd: FileDescriptorLike, /) -> None:
        """Grant access to the slave pseudo-terminal device.

          fd
            File descriptor of a master pseudo-terminal device.

        Performs a grantpt() C function call.
        """

    def unlockpt(fd: FileDescriptorLike, /) -> None:
        """Unlock a pseudo-terminal master/slave pair.

          fd
            File descriptor of a master pseudo-terminal device.

        Performs an unlockpt() C function call.
        """

    def ptsname(fd: FileDescriptorLike, /) -> str:
        """Return the name of the slave pseudo-terminal device.

          fd
            File descriptor of a master pseudo-terminal device.

        If the ptsname_r() C function is available, it is called;
        otherwise, performs a ptsname() C function call.
        """

if sys.version_info >= (3, 13) and sys.platform == "linux":
    TFD_TIMER_ABSTIME: Final = 1
    TFD_TIMER_CANCEL_ON_SET: Final = 2
    TFD_NONBLOCK: Final[int]
    TFD_CLOEXEC: Final[int]
    POSIX_SPAWN_CLOSEFROM: Final[int]

    def timerfd_create(clockid: int, /, *, flags: int = 0) -> int:
        """Create and return a timer file descriptor.

        clockid
          A valid clock ID constant as timer file descriptor.

          time.CLOCK_REALTIME
          time.CLOCK_MONOTONIC
          time.CLOCK_BOOTTIME
        flags
          0 or a bit mask of os.TFD_NONBLOCK or os.TFD_CLOEXEC.

          os.TFD_NONBLOCK
              If *TFD_NONBLOCK* is set as a flag, read doesn't blocks.
              If *TFD_NONBLOCK* is not set as a flag, read block until the timer fires.

          os.TFD_CLOEXEC
              If *TFD_CLOEXEC* is set as a flag, enable the close-on-exec flag
        """

    def timerfd_settime(
        fd: FileDescriptor, /, *, flags: int = 0, initial: float = 0.0, interval: float = 0.0
    ) -> tuple[float, float]:
        """Alter a timer file descriptor's internal timer in seconds.

        fd
          A timer file descriptor.
        flags
          0 or a bit mask of TFD_TIMER_ABSTIME or TFD_TIMER_CANCEL_ON_SET.
        initial
          The initial expiration time, in seconds.
        interval
          The timer's interval, in seconds.
        """

    def timerfd_settime_ns(fd: FileDescriptor, /, *, flags: int = 0, initial: int = 0, interval: int = 0) -> tuple[int, int]:
        """Alter a timer file descriptor's internal timer in nanoseconds.

        fd
          A timer file descriptor.
        flags
          0 or a bit mask of TFD_TIMER_ABSTIME or TFD_TIMER_CANCEL_ON_SET.
        initial
          initial expiration timing in seconds.
        interval
          interval for the timer in seconds.
        """

    def timerfd_gettime(fd: FileDescriptor, /) -> tuple[float, float]:
        """Return a tuple of a timer file descriptor's (interval, next expiration) in float seconds.

        fd
          A timer file descriptor.
        """

    def timerfd_gettime_ns(fd: FileDescriptor, /) -> tuple[int, int]:
        """Return a tuple of a timer file descriptor's (interval, next expiration) in nanoseconds.

        fd
          A timer file descriptor.
        """

if sys.version_info >= (3, 13) or sys.platform != "win32":
    # Added to Windows in 3.13.
    def fchmod(fd: int, mode: int) -> None:
        """Change the access permissions of the file given by file descriptor fd.

          fd
            The file descriptor of the file to be modified.
          mode
            Operating-system mode bitfield.
            Be careful when using number literals for *mode*. The conventional UNIX notation for
            numeric modes uses an octal base, which needs to be indicated with a ``0o`` prefix in
            Python.

        Equivalent to os.chmod(fd, mode).
        """

if sys.platform != "linux":
    if sys.version_info >= (3, 13) or sys.platform != "win32":
        # Added to Windows in 3.13.
        def lchmod(path: StrOrBytesPath, mode: int) -> None:
            """Change the access permissions of a file, without following symbolic links.

            If path is a symlink, this affects the link itself rather than the target.
            Equivalent to chmod(path, mode, follow_symlinks=False)."
            """
