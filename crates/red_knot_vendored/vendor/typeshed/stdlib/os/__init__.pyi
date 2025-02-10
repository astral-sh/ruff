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
from types import TracebackType
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
)
from typing_extensions import Self, TypeAlias, Unpack, deprecated

from . import path as _path

if sys.version_info >= (3, 9):
    from types import GenericAlias

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
    "urandom",
    "utime",
    "waitpid",
    "walk",
    "write",
]
if sys.version_info >= (3, 9):
    __all__ += ["waitstatus_to_exitcode"]
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
        "removexattr",
        "setxattr",
    ]
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
if sys.platform == "linux" and sys.version_info >= (3, 9):
    __all__ += ["P_PIDFD", "pidfd_open"]
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
if sys.platform != "win32" and sys.version_info >= (3, 9):
    __all__ += ["CLD_KILLED", "CLD_STOPPED"]
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
if sys.platform != "win32" or sys.version_info >= (3, 9):
    __all__ += ["unsetenv"]

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
    PRIO_PROCESS: int
    PRIO_PGRP: int
    PRIO_USER: int

    F_LOCK: int
    F_TLOCK: int
    F_ULOCK: int
    F_TEST: int

    if sys.platform != "darwin":
        POSIX_FADV_NORMAL: int
        POSIX_FADV_SEQUENTIAL: int
        POSIX_FADV_RANDOM: int
        POSIX_FADV_NOREUSE: int
        POSIX_FADV_WILLNEED: int
        POSIX_FADV_DONTNEED: int

    if sys.platform != "linux" and sys.platform != "darwin":
        # In the os-module docs, these are marked as being available
        # on "Unix, not Emscripten, not WASI."
        # However, in the source code, a comment indicates they're "FreeBSD constants".
        # sys.platform could have one of many values on a FreeBSD Python build,
        # so the sys-module docs recommend doing `if sys.platform.startswith('freebsd')`
        # to detect FreeBSD builds. Unfortunately that would be too dynamic
        # for type checkers, however.
        SF_NODISKIO: int
        SF_MNOWAIT: int
        SF_SYNC: int

        if sys.version_info >= (3, 11):
            SF_NOCACHE: int

    if sys.platform == "linux":
        XATTR_SIZE_MAX: int
        XATTR_CREATE: int
        XATTR_REPLACE: int

    P_PID: int
    P_PGID: int
    P_ALL: int

    if sys.platform == "linux" and sys.version_info >= (3, 9):
        P_PIDFD: int

    WEXITED: int
    WSTOPPED: int
    WNOWAIT: int

    CLD_EXITED: int
    CLD_DUMPED: int
    CLD_TRAPPED: int
    CLD_CONTINUED: int

    if sys.version_info >= (3, 9):
        CLD_KILLED: int
        CLD_STOPPED: int

    SCHED_OTHER: int
    SCHED_FIFO: int
    SCHED_RR: int
    if sys.platform != "darwin" and sys.platform != "linux":
        SCHED_SPORADIC: int

if sys.platform == "linux":
    SCHED_BATCH: int
    SCHED_IDLE: int
    SCHED_RESET_ON_FORK: int

if sys.platform != "win32":
    RTLD_LAZY: int
    RTLD_NOW: int
    RTLD_GLOBAL: int
    RTLD_LOCAL: int
    RTLD_NODELETE: int
    RTLD_NOLOAD: int

if sys.platform == "linux":
    RTLD_DEEPBIND: int
    GRND_NONBLOCK: int
    GRND_RANDOM: int

if sys.platform == "darwin" and sys.version_info >= (3, 12):
    PRIO_DARWIN_BG: int
    PRIO_DARWIN_NONUI: int
    PRIO_DARWIN_PROCESS: int
    PRIO_DARWIN_THREAD: int

SEEK_SET: int
SEEK_CUR: int
SEEK_END: int
if sys.platform != "win32":
    SEEK_DATA: int
    SEEK_HOLE: int

O_RDONLY: int
O_WRONLY: int
O_RDWR: int
O_APPEND: int
O_CREAT: int
O_EXCL: int
O_TRUNC: int
if sys.platform == "win32":
    O_BINARY: int
    O_NOINHERIT: int
    O_SHORT_LIVED: int
    O_TEMPORARY: int
    O_RANDOM: int
    O_SEQUENTIAL: int
    O_TEXT: int

if sys.platform != "win32":
    O_DSYNC: int
    O_SYNC: int
    O_NDELAY: int
    O_NONBLOCK: int
    O_NOCTTY: int
    O_CLOEXEC: int
    O_ASYNC: int  # Gnu extension if in C library
    O_DIRECTORY: int  # Gnu extension if in C library
    O_NOFOLLOW: int  # Gnu extension if in C library
    O_ACCMODE: int  # TODO: when does this exist?

if sys.platform == "linux":
    O_RSYNC: int
    O_DIRECT: int  # Gnu extension if in C library
    O_NOATIME: int  # Gnu extension if in C library
    O_PATH: int  # Gnu extension if in C library
    O_TMPFILE: int  # Gnu extension if in C library
    O_LARGEFILE: int  # Gnu extension if in C library

if sys.platform != "linux" and sys.platform != "win32":
    O_SHLOCK: int
    O_EXLOCK: int

if sys.platform == "darwin" and sys.version_info >= (3, 10):
    O_EVTONLY: int
    O_NOFOLLOW_ANY: int
    O_SYMLINK: int

if sys.platform != "win32" and sys.version_info >= (3, 10):
    O_FSYNC: int

if sys.platform != "linux" and sys.platform != "win32" and sys.version_info >= (3, 13):
    O_EXEC: int
    O_SEARCH: int

if sys.platform != "win32" and sys.platform != "darwin":
    # posix, but apparently missing on macos
    ST_APPEND: int
    ST_MANDLOCK: int
    ST_NOATIME: int
    ST_NODEV: int
    ST_NODIRATIME: int
    ST_NOEXEC: int
    ST_RELATIME: int
    ST_SYNCHRONOUS: int
    ST_WRITE: int

if sys.platform != "win32":
    NGROUPS_MAX: int
    ST_NOSUID: int
    ST_RDONLY: int

curdir: str
pardir: str
sep: str
if sys.platform == "win32":
    altsep: str
else:
    altsep: str | None
extsep: str
pathsep: str
defpath: str
linesep: str
devnull: str
name: str

F_OK: int
R_OK: int
W_OK: int
X_OK: int

_EnvironCodeFunc: TypeAlias = Callable[[AnyStr], AnyStr]

class _Environ(MutableMapping[AnyStr, AnyStr], Generic[AnyStr]):
    encodekey: _EnvironCodeFunc[AnyStr]
    decodekey: _EnvironCodeFunc[AnyStr]
    encodevalue: _EnvironCodeFunc[AnyStr]
    decodevalue: _EnvironCodeFunc[AnyStr]
    if sys.version_info >= (3, 9):
        def __init__(
            self,
            data: MutableMapping[AnyStr, AnyStr],
            encodekey: _EnvironCodeFunc[AnyStr],
            decodekey: _EnvironCodeFunc[AnyStr],
            encodevalue: _EnvironCodeFunc[AnyStr],
            decodevalue: _EnvironCodeFunc[AnyStr],
        ) -> None: ...
    else:
        putenv: Callable[[AnyStr, AnyStr], object]
        unsetenv: Callable[[AnyStr, AnyStr], object]
        def __init__(
            self,
            data: MutableMapping[AnyStr, AnyStr],
            encodekey: _EnvironCodeFunc[AnyStr],
            decodekey: _EnvironCodeFunc[AnyStr],
            encodevalue: _EnvironCodeFunc[AnyStr],
            decodevalue: _EnvironCodeFunc[AnyStr],
            putenv: Callable[[AnyStr, AnyStr], object],
            unsetenv: Callable[[AnyStr, AnyStr], object],
        ) -> None: ...

    def setdefault(self, key: AnyStr, value: AnyStr) -> AnyStr: ...
    def copy(self) -> dict[AnyStr, AnyStr]: ...
    def __delitem__(self, key: AnyStr) -> None: ...
    def __getitem__(self, key: AnyStr) -> AnyStr: ...
    def __setitem__(self, key: AnyStr, value: AnyStr) -> None: ...
    def __iter__(self) -> Iterator[AnyStr]: ...
    def __len__(self) -> int: ...
    if sys.version_info >= (3, 9):
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

if sys.version_info >= (3, 11) or sys.platform != "win32":
    EX_OK: int

if sys.platform != "win32":
    confstr_names: dict[str, int]
    pathconf_names: dict[str, int]
    sysconf_names: dict[str, int]

    EX_USAGE: int
    EX_DATAERR: int
    EX_NOINPUT: int
    EX_NOUSER: int
    EX_NOHOST: int
    EX_UNAVAILABLE: int
    EX_SOFTWARE: int
    EX_OSERR: int
    EX_OSFILE: int
    EX_CANTCREAT: int
    EX_IOERR: int
    EX_TEMPFAIL: int
    EX_PROTOCOL: int
    EX_NOPERM: int
    EX_CONFIG: int

# Exists on some Unix platforms, e.g. Solaris.
if sys.platform != "win32" and sys.platform != "darwin" and sys.platform != "linux":
    EX_NOTFOUND: int

P_NOWAIT: int
P_NOWAITO: int
P_WAIT: int
if sys.platform == "win32":
    P_DETACH: int
    P_OVERLAY: int

# wait()/waitpid() options
if sys.platform != "win32":
    WNOHANG: int  # Unix only
    WCONTINUED: int  # some Unix systems
    WUNTRACED: int  # Unix only

TMP_MAX: int  # Undocumented, but used by tempfile

# ----- os classes (structures) -----
@final
class stat_result(structseq[float], tuple[int, int, int, int, int, int, int, float, float, float]):
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
    def st_mode(self) -> int: ...  # protection bits,
    @property
    def st_ino(self) -> int: ...  # inode number,
    @property
    def st_dev(self) -> int: ...  # device,
    @property
    def st_nlink(self) -> int: ...  # number of hard links,
    @property
    def st_uid(self) -> int: ...  # user id of owner,
    @property
    def st_gid(self) -> int: ...  # group id of owner,
    @property
    def st_size(self) -> int: ...  # size of file, in bytes,
    @property
    def st_atime(self) -> float: ...  # time of most recent access,
    @property
    def st_mtime(self) -> float: ...  # time of most recent content modification,
    # platform dependent (time of most recent metadata change on Unix, or the time of creation on Windows)
    if sys.version_info >= (3, 12) and sys.platform == "win32":
        @property
        @deprecated(
            """\
Use st_birthtime instead to retrieve the file creation time. \
In the future, this property will contain the last metadata change time."""
        )
        def st_ctime(self) -> float: ...
    else:
        @property
        def st_ctime(self) -> float: ...

    @property
    def st_atime_ns(self) -> int: ...  # time of most recent access, in nanoseconds
    @property
    def st_mtime_ns(self) -> int: ...  # time of most recent content modification in nanoseconds
    # platform dependent (time of most recent metadata change on Unix, or the time of creation on Windows) in nanoseconds
    @property
    def st_ctime_ns(self) -> int: ...
    if sys.platform == "win32":
        @property
        def st_file_attributes(self) -> int: ...
        @property
        def st_reparse_tag(self) -> int: ...
        if sys.version_info >= (3, 12):
            @property
            def st_birthtime(self) -> float: ...  # time of file creation in seconds
            @property
            def st_birthtime_ns(self) -> int: ...  # time of file creation in nanoseconds
    else:
        @property
        def st_blocks(self) -> int: ...  # number of blocks allocated for file
        @property
        def st_blksize(self) -> int: ...  # filesystem blocksize
        @property
        def st_rdev(self) -> int: ...  # type of device if an inode device
        if sys.platform != "linux":
            # These properties are available on MacOS, but not Ubuntu.
            # On other Unix systems (such as FreeBSD), the following attributes may be
            # available (but may be only filled out if root tries to use them):
            @property
            def st_gen(self) -> int: ...  # file generation number
            @property
            def st_birthtime(self) -> float: ...  # time of file creation in seconds
    if sys.platform == "darwin":
        @property
        def st_flags(self) -> int: ...  # user defined flags for file
    # Attributes documented as sometimes appearing, but deliberately omitted from the stub: `st_creator`, `st_rsize`, `st_type`.
    # See https://github.com/python/typeshed/pull/6560#issuecomment-991253327

# mypy and pyright object to this being both ABC and Protocol.
# At runtime it inherits from ABC and is not a Protocol, but it will be
# on the allowlist for use as a Protocol starting in 3.14.
@runtime_checkable
class PathLike(ABC, Protocol[AnyStr_co]):  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
    @abstractmethod
    def __fspath__(self) -> AnyStr_co: ...

@overload
def listdir(path: StrPath | None = None) -> list[str]: ...
@overload
def listdir(path: BytesPath) -> list[bytes]: ...
@overload
def listdir(path: int) -> list[str]: ...
@final
class DirEntry(Generic[AnyStr]):
    # This is what the scandir iterator yields
    # The constructor is hidden

    @property
    def name(self) -> AnyStr: ...
    @property
    def path(self) -> AnyStr: ...
    def inode(self) -> int: ...
    def is_dir(self, *, follow_symlinks: bool = True) -> bool: ...
    def is_file(self, *, follow_symlinks: bool = True) -> bool: ...
    def is_symlink(self) -> bool: ...
    def stat(self, *, follow_symlinks: bool = True) -> stat_result: ...
    def __fspath__(self) -> AnyStr: ...
    if sys.version_info >= (3, 9):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...
    if sys.version_info >= (3, 12):
        def is_junction(self) -> bool: ...

@final
class statvfs_result(structseq[int], tuple[int, int, int, int, int, int, int, int, int, int, int]):
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
def fsencode(filename: StrOrBytesPath) -> bytes: ...
def fsdecode(filename: StrOrBytesPath) -> str: ...
@overload
def fspath(path: str) -> str: ...
@overload
def fspath(path: bytes) -> bytes: ...
@overload
def fspath(path: PathLike[AnyStr]) -> AnyStr: ...
def get_exec_path(env: Mapping[str, str] | None = None) -> list[str]: ...
def getlogin() -> str: ...
def getpid() -> int: ...
def getppid() -> int: ...
def strerror(code: int, /) -> str: ...
def umask(mask: int, /) -> int: ...
@final
class uname_result(structseq[str], tuple[str, str, str, str, str]):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("sysname", "nodename", "release", "version", "machine")

    @property
    def sysname(self) -> str: ...
    @property
    def nodename(self) -> str: ...
    @property
    def release(self) -> str: ...
    @property
    def version(self) -> str: ...
    @property
    def machine(self) -> str: ...

if sys.platform != "win32":
    def ctermid() -> str: ...
    def getegid() -> int: ...
    def geteuid() -> int: ...
    def getgid() -> int: ...
    def getgrouplist(user: str, group: int, /) -> list[int]: ...
    def getgroups() -> list[int]: ...  # Unix only, behaves differently on Mac
    def initgroups(username: str, gid: int, /) -> None: ...
    def getpgid(pid: int) -> int: ...
    def getpgrp() -> int: ...
    def getpriority(which: int, who: int) -> int: ...
    def setpriority(which: int, who: int, priority: int) -> None: ...
    if sys.platform != "darwin":
        def getresuid() -> tuple[int, int, int]: ...
        def getresgid() -> tuple[int, int, int]: ...

    def getuid() -> int: ...
    def setegid(egid: int, /) -> None: ...
    def seteuid(euid: int, /) -> None: ...
    def setgid(gid: int, /) -> None: ...
    def setgroups(groups: Sequence[int], /) -> None: ...
    def setpgrp() -> None: ...
    def setpgid(pid: int, pgrp: int, /) -> None: ...
    def setregid(rgid: int, egid: int, /) -> None: ...
    if sys.platform != "darwin":
        def setresgid(rgid: int, egid: int, sgid: int, /) -> None: ...
        def setresuid(ruid: int, euid: int, suid: int, /) -> None: ...

    def setreuid(ruid: int, euid: int, /) -> None: ...
    def getsid(pid: int, /) -> int: ...
    def setsid() -> None: ...
    def setuid(uid: int, /) -> None: ...
    def uname() -> uname_result: ...

@overload
def getenv(key: str) -> str | None: ...
@overload
def getenv(key: str, default: _T) -> str | _T: ...

if sys.platform != "win32":
    @overload
    def getenvb(key: bytes) -> bytes | None: ...
    @overload
    def getenvb(key: bytes, default: _T) -> bytes | _T: ...
    def putenv(name: StrOrBytesPath, value: StrOrBytesPath, /) -> None: ...
    def unsetenv(name: StrOrBytesPath, /) -> None: ...

else:
    def putenv(name: str, value: str, /) -> None: ...

    if sys.version_info >= (3, 9):
        def unsetenv(name: str, /) -> None: ...

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
def close(fd: int) -> None: ...
def closerange(fd_low: int, fd_high: int, /) -> None: ...
def device_encoding(fd: int) -> str | None: ...
def dup(fd: int, /) -> int: ...
def dup2(fd: int, fd2: int, inheritable: bool = True) -> int: ...
def fstat(fd: int) -> stat_result: ...
def ftruncate(fd: int, length: int, /) -> None: ...
def fsync(fd: FileDescriptorLike) -> None: ...
def isatty(fd: int, /) -> bool: ...

if sys.platform != "win32" and sys.version_info >= (3, 11):
    def login_tty(fd: int, /) -> None: ...

if sys.version_info >= (3, 11):
    def lseek(fd: int, position: int, whence: int, /) -> int: ...

else:
    def lseek(fd: int, position: int, how: int, /) -> int: ...

def open(path: StrOrBytesPath, flags: int, mode: int = 0o777, *, dir_fd: int | None = None) -> int: ...
def pipe() -> tuple[int, int]: ...
def read(fd: int, length: int, /) -> bytes: ...

if sys.version_info >= (3, 12) or sys.platform != "win32":
    def get_blocking(fd: int, /) -> bool: ...
    def set_blocking(fd: int, blocking: bool, /) -> None: ...

if sys.platform != "win32":
    def fchown(fd: int, uid: int, gid: int) -> None: ...
    def fpathconf(fd: int, name: str | int, /) -> int: ...
    def fstatvfs(fd: int, /) -> statvfs_result: ...
    def lockf(fd: int, command: int, length: int, /) -> None: ...
    def openpty() -> tuple[int, int]: ...  # some flavors of Unix
    if sys.platform != "darwin":
        def fdatasync(fd: FileDescriptorLike) -> None: ...
        def pipe2(flags: int, /) -> tuple[int, int]: ...  # some flavors of Unix
        def posix_fallocate(fd: int, offset: int, length: int, /) -> None: ...
        def posix_fadvise(fd: int, offset: int, length: int, advice: int, /) -> None: ...

    def pread(fd: int, length: int, offset: int, /) -> bytes: ...
    def pwrite(fd: int, buffer: ReadableBuffer, offset: int, /) -> int: ...
    # In CI, stubtest sometimes reports that these are available on MacOS, sometimes not
    def preadv(fd: int, buffers: SupportsLenAndGetItem[WriteableBuffer], offset: int, flags: int = 0, /) -> int: ...
    def pwritev(fd: int, buffers: SupportsLenAndGetItem[ReadableBuffer], offset: int, flags: int = 0, /) -> int: ...
    if sys.platform != "darwin":
        if sys.version_info >= (3, 10):
            RWF_APPEND: int  # docs say available on 3.7+, stubtest says otherwise
        RWF_DSYNC: int
        RWF_SYNC: int
        RWF_HIPRI: int
        RWF_NOWAIT: int

    if sys.platform == "linux":
        def sendfile(out_fd: FileDescriptor, in_fd: FileDescriptor, offset: int | None, count: int) -> int: ...
    else:
        def sendfile(
            out_fd: FileDescriptor,
            in_fd: FileDescriptor,
            offset: int,
            count: int,
            headers: Sequence[ReadableBuffer] = ...,
            trailers: Sequence[ReadableBuffer] = ...,
            flags: int = 0,
        ) -> int: ...  # FreeBSD and Mac OS X only

    def readv(fd: int, buffers: SupportsLenAndGetItem[WriteableBuffer], /) -> int: ...
    def writev(fd: int, buffers: SupportsLenAndGetItem[ReadableBuffer], /) -> int: ...

@final
class terminal_size(structseq[int], tuple[int, int]):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("columns", "lines")

    @property
    def columns(self) -> int: ...
    @property
    def lines(self) -> int: ...

def get_terminal_size(fd: int = ..., /) -> terminal_size: ...
def get_inheritable(fd: int, /) -> bool: ...
def set_inheritable(fd: int, inheritable: bool, /) -> None: ...

if sys.platform == "win32":
    def get_handle_inheritable(handle: int, /) -> bool: ...
    def set_handle_inheritable(handle: int, inheritable: bool, /) -> None: ...

if sys.platform != "win32":
    # Unix only
    def tcgetpgrp(fd: int, /) -> int: ...
    def tcsetpgrp(fd: int, pgid: int, /) -> None: ...
    def ttyname(fd: int, /) -> str: ...

def write(fd: int, data: ReadableBuffer, /) -> int: ...
def access(
    path: FileDescriptorOrPath, mode: int, *, dir_fd: int | None = None, effective_ids: bool = False, follow_symlinks: bool = True
) -> bool: ...
def chdir(path: FileDescriptorOrPath) -> None: ...

if sys.platform != "win32":
    def fchdir(fd: FileDescriptorLike) -> None: ...

def getcwd() -> str: ...
def getcwdb() -> bytes: ...
def chmod(path: FileDescriptorOrPath, mode: int, *, dir_fd: int | None = None, follow_symlinks: bool = ...) -> None: ...

if sys.platform != "win32" and sys.platform != "linux":
    def chflags(path: StrOrBytesPath, flags: int, follow_symlinks: bool = True) -> None: ...  # some flavors of Unix
    def lchflags(path: StrOrBytesPath, flags: int) -> None: ...

if sys.platform != "win32":
    def chroot(path: StrOrBytesPath) -> None: ...
    def chown(
        path: FileDescriptorOrPath, uid: int, gid: int, *, dir_fd: int | None = None, follow_symlinks: bool = True
    ) -> None: ...
    def lchown(path: StrOrBytesPath, uid: int, gid: int) -> None: ...

def link(
    src: StrOrBytesPath,
    dst: StrOrBytesPath,
    *,
    src_dir_fd: int | None = None,
    dst_dir_fd: int | None = None,
    follow_symlinks: bool = True,
) -> None: ...
def lstat(path: StrOrBytesPath, *, dir_fd: int | None = None) -> stat_result: ...
def mkdir(path: StrOrBytesPath, mode: int = 0o777, *, dir_fd: int | None = None) -> None: ...

if sys.platform != "win32":
    def mkfifo(path: StrOrBytesPath, mode: int = 0o666, *, dir_fd: int | None = None) -> None: ...  # Unix only

def makedirs(name: StrOrBytesPath, mode: int = 0o777, exist_ok: bool = False) -> None: ...

if sys.platform != "win32":
    def mknod(path: StrOrBytesPath, mode: int = 0o600, device: int = 0, *, dir_fd: int | None = None) -> None: ...
    def major(device: int, /) -> int: ...
    def minor(device: int, /) -> int: ...
    def makedev(major: int, minor: int, /) -> int: ...
    def pathconf(path: FileDescriptorOrPath, name: str | int) -> int: ...  # Unix only

def readlink(path: GenericPath[AnyStr], *, dir_fd: int | None = None) -> AnyStr: ...
def remove(path: StrOrBytesPath, *, dir_fd: int | None = None) -> None: ...
def removedirs(name: StrOrBytesPath) -> None: ...
def rename(src: StrOrBytesPath, dst: StrOrBytesPath, *, src_dir_fd: int | None = None, dst_dir_fd: int | None = None) -> None: ...
def renames(old: StrOrBytesPath, new: StrOrBytesPath) -> None: ...
def replace(
    src: StrOrBytesPath, dst: StrOrBytesPath, *, src_dir_fd: int | None = None, dst_dir_fd: int | None = None
) -> None: ...
def rmdir(path: StrOrBytesPath, *, dir_fd: int | None = None) -> None: ...
@final
class _ScandirIterator(Generic[AnyStr]):
    def __del__(self) -> None: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> DirEntry[AnyStr]: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, *args: Unused) -> None: ...
    def close(self) -> None: ...

@overload
def scandir(path: None = None) -> _ScandirIterator[str]: ...
@overload
def scandir(path: int) -> _ScandirIterator[str]: ...
@overload
def scandir(path: GenericPath[AnyStr]) -> _ScandirIterator[AnyStr]: ...
def stat(path: FileDescriptorOrPath, *, dir_fd: int | None = None, follow_symlinks: bool = True) -> stat_result: ...

if sys.platform != "win32":
    def statvfs(path: FileDescriptorOrPath) -> statvfs_result: ...  # Unix only

def symlink(
    src: StrOrBytesPath, dst: StrOrBytesPath, target_is_directory: bool = False, *, dir_fd: int | None = None
) -> None: ...

if sys.platform != "win32":
    def sync() -> None: ...  # Unix only

def truncate(path: FileDescriptorOrPath, length: int) -> None: ...  # Unix only up to version 3.4
def unlink(path: StrOrBytesPath, *, dir_fd: int | None = None) -> None: ...
def utime(
    path: FileDescriptorOrPath,
    times: tuple[int, int] | tuple[float, float] | None = None,
    *,
    ns: tuple[int, int] = ...,
    dir_fd: int | None = None,
    follow_symlinks: bool = True,
) -> None: ...

_OnError: TypeAlias = Callable[[OSError], object]

def walk(
    top: GenericPath[AnyStr], topdown: bool = True, onerror: _OnError | None = None, followlinks: bool = False
) -> Iterator[tuple[AnyStr, list[AnyStr], list[AnyStr]]]: ...

if sys.platform != "win32":
    @overload
    def fwalk(
        top: StrPath = ".",
        topdown: bool = True,
        onerror: _OnError | None = None,
        *,
        follow_symlinks: bool = False,
        dir_fd: int | None = None,
    ) -> Iterator[tuple[str, list[str], list[str], int]]: ...
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
        def getxattr(path: FileDescriptorOrPath, attribute: StrOrBytesPath, *, follow_symlinks: bool = True) -> bytes: ...
        def listxattr(path: FileDescriptorOrPath | None = None, *, follow_symlinks: bool = True) -> list[str]: ...
        def removexattr(path: FileDescriptorOrPath, attribute: StrOrBytesPath, *, follow_symlinks: bool = True) -> None: ...
        def setxattr(
            path: FileDescriptorOrPath,
            attribute: StrOrBytesPath,
            value: ReadableBuffer,
            flags: int = 0,
            *,
            follow_symlinks: bool = True,
        ) -> None: ...

def abort() -> NoReturn: ...

# These are defined as execl(file, *args) but the first *arg is mandatory.
def execl(file: StrOrBytesPath, *args: Unpack[tuple[StrOrBytesPath, Unpack[tuple[StrOrBytesPath, ...]]]]) -> NoReturn: ...
def execlp(file: StrOrBytesPath, *args: Unpack[tuple[StrOrBytesPath, Unpack[tuple[StrOrBytesPath, ...]]]]) -> NoReturn: ...

# These are: execle(file, *args, env) but env is pulled from the last element of the args.
def execle(
    file: StrOrBytesPath, *args: Unpack[tuple[StrOrBytesPath, Unpack[tuple[StrOrBytesPath, ...]], _ExecEnv]]
) -> NoReturn: ...
def execlpe(
    file: StrOrBytesPath, *args: Unpack[tuple[StrOrBytesPath, Unpack[tuple[StrOrBytesPath, ...]], _ExecEnv]]
) -> NoReturn: ...

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

def execv(path: StrOrBytesPath, argv: _ExecVArgs, /) -> NoReturn: ...
def execve(path: FileDescriptorOrPath, argv: _ExecVArgs, env: _ExecEnv) -> NoReturn: ...
def execvp(file: StrOrBytesPath, args: _ExecVArgs) -> NoReturn: ...
def execvpe(file: StrOrBytesPath, args: _ExecVArgs, env: _ExecEnv) -> NoReturn: ...
def _exit(status: int) -> NoReturn: ...
def kill(pid: int, signal: int, /) -> None: ...

if sys.platform != "win32":
    # Unix only
    def fork() -> int: ...
    def forkpty() -> tuple[int, int]: ...  # some flavors of Unix
    def killpg(pgid: int, signal: int, /) -> None: ...
    def nice(increment: int, /) -> int: ...
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
def spawnl(mode: int, file: StrOrBytesPath, arg0: StrOrBytesPath, *args: StrOrBytesPath) -> int: ...
def spawnle(mode: int, file: StrOrBytesPath, arg0: StrOrBytesPath, *args: Any) -> int: ...  # Imprecise sig

if sys.platform != "win32":
    def spawnv(mode: int, file: StrOrBytesPath, args: _ExecVArgs) -> int: ...
    def spawnve(mode: int, file: StrOrBytesPath, args: _ExecVArgs, env: _ExecEnv) -> int: ...

else:
    def spawnv(mode: int, path: StrOrBytesPath, argv: _ExecVArgs, /) -> int: ...
    def spawnve(mode: int, path: StrOrBytesPath, argv: _ExecVArgs, env: _ExecEnv, /) -> int: ...

def system(command: StrOrBytesPath) -> int: ...
@final
class times_result(structseq[float], tuple[float, float, float, float, float]):
    if sys.version_info >= (3, 10):
        __match_args__: Final = ("user", "system", "children_user", "children_system", "elapsed")

    @property
    def user(self) -> float: ...
    @property
    def system(self) -> float: ...
    @property
    def children_user(self) -> float: ...
    @property
    def children_system(self) -> float: ...
    @property
    def elapsed(self) -> float: ...

def times() -> times_result: ...
def waitpid(pid: int, options: int, /) -> tuple[int, int]: ...

if sys.platform == "win32":
    if sys.version_info >= (3, 10):
        def startfile(
            filepath: StrOrBytesPath,
            operation: str = ...,
            arguments: str = "",
            cwd: StrOrBytesPath | None = None,
            show_cmd: int = 1,
        ) -> None: ...
    else:
        def startfile(filepath: StrOrBytesPath, operation: str = ...) -> None: ...

else:
    def spawnlp(mode: int, file: StrOrBytesPath, arg0: StrOrBytesPath, *args: StrOrBytesPath) -> int: ...
    def spawnlpe(mode: int, file: StrOrBytesPath, arg0: StrOrBytesPath, *args: Any) -> int: ...  # Imprecise signature
    def spawnvp(mode: int, file: StrOrBytesPath, args: _ExecVArgs) -> int: ...
    def spawnvpe(mode: int, file: StrOrBytesPath, args: _ExecVArgs, env: _ExecEnv) -> int: ...
    def wait() -> tuple[int, int]: ...  # Unix only
    # Added to MacOS in 3.13
    if sys.platform != "darwin" or sys.version_info >= (3, 13):
        @final
        class waitid_result(structseq[int], tuple[int, int, int, int, int]):
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

        def waitid(idtype: int, ident: int, options: int, /) -> waitid_result | None: ...

    from resource import struct_rusage

    def wait3(options: int) -> tuple[int, int, struct_rusage]: ...
    def wait4(pid: int, options: int) -> tuple[int, int, struct_rusage]: ...
    def WCOREDUMP(status: int, /) -> bool: ...
    def WIFCONTINUED(status: int) -> bool: ...
    def WIFSTOPPED(status: int) -> bool: ...
    def WIFSIGNALED(status: int) -> bool: ...
    def WIFEXITED(status: int) -> bool: ...
    def WEXITSTATUS(status: int) -> int: ...
    def WSTOPSIG(status: int) -> int: ...
    def WTERMSIG(status: int) -> int: ...
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
    ) -> int: ...
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
    ) -> int: ...
    POSIX_SPAWN_OPEN: int
    POSIX_SPAWN_CLOSE: int
    POSIX_SPAWN_DUP2: int

if sys.platform != "win32":
    @final
    class sched_param(structseq[int], tuple[int]):
        if sys.version_info >= (3, 10):
            __match_args__: Final = ("sched_priority",)

        def __new__(cls, sched_priority: int) -> Self: ...
        @property
        def sched_priority(self) -> int: ...

    def sched_get_priority_min(policy: int) -> int: ...  # some flavors of Unix
    def sched_get_priority_max(policy: int) -> int: ...  # some flavors of Unix
    def sched_yield() -> None: ...  # some flavors of Unix
    if sys.platform != "darwin":
        def sched_setscheduler(pid: int, policy: int, param: sched_param, /) -> None: ...  # some flavors of Unix
        def sched_getscheduler(pid: int, /) -> int: ...  # some flavors of Unix
        def sched_rr_get_interval(pid: int, /) -> float: ...  # some flavors of Unix
        def sched_setparam(pid: int, param: sched_param, /) -> None: ...  # some flavors of Unix
        def sched_getparam(pid: int, /) -> sched_param: ...  # some flavors of Unix
        def sched_setaffinity(pid: int, mask: Iterable[int], /) -> None: ...  # some flavors of Unix
        def sched_getaffinity(pid: int, /) -> set[int]: ...  # some flavors of Unix

def cpu_count() -> int | None: ...

if sys.version_info >= (3, 13):
    # Documented to return `int | None`, but falls back to `len(sched_getaffinity(0))` when
    # available. See https://github.com/python/cpython/blob/417c130/Lib/os.py#L1175-L1186.
    if sys.platform != "win32" and sys.platform != "darwin":
        def process_cpu_count() -> int: ...
    else:
        def process_cpu_count() -> int | None: ...

if sys.platform != "win32":
    # Unix only
    def confstr(name: str | int, /) -> str | None: ...
    def getloadavg() -> tuple[float, float, float]: ...
    def sysconf(name: str | int, /) -> int: ...

if sys.platform == "linux":
    def getrandom(size: int, flags: int = 0) -> bytes: ...

def urandom(size: int, /) -> bytes: ...

if sys.platform != "win32":
    def register_at_fork(
        *,
        before: Callable[..., Any] | None = ...,
        after_in_parent: Callable[..., Any] | None = ...,
        after_in_child: Callable[..., Any] | None = ...,
    ) -> None: ...

if sys.platform == "win32":
    class _AddedDllDirectory:
        path: str | None
        def __init__(self, path: str | None, cookie: _T, remove_dll_directory: Callable[[_T], object]) -> None: ...
        def close(self) -> None: ...
        def __enter__(self) -> Self: ...
        def __exit__(self, *args: Unused) -> None: ...

    def add_dll_directory(path: str) -> _AddedDllDirectory: ...

if sys.platform == "linux":
    MFD_CLOEXEC: int
    MFD_ALLOW_SEALING: int
    MFD_HUGETLB: int
    MFD_HUGE_SHIFT: int
    MFD_HUGE_MASK: int
    MFD_HUGE_64KB: int
    MFD_HUGE_512KB: int
    MFD_HUGE_1MB: int
    MFD_HUGE_2MB: int
    MFD_HUGE_8MB: int
    MFD_HUGE_16MB: int
    MFD_HUGE_32MB: int
    MFD_HUGE_256MB: int
    MFD_HUGE_512MB: int
    MFD_HUGE_1GB: int
    MFD_HUGE_2GB: int
    MFD_HUGE_16GB: int
    def memfd_create(name: str, flags: int = ...) -> int: ...
    def copy_file_range(src: int, dst: int, count: int, offset_src: int | None = ..., offset_dst: int | None = ...) -> int: ...

if sys.version_info >= (3, 9):
    def waitstatus_to_exitcode(status: int) -> int: ...

    if sys.platform == "linux":
        def pidfd_open(pid: int, flags: int = ...) -> int: ...

if sys.version_info >= (3, 12) and sys.platform == "win32":
    def listdrives() -> list[str]: ...
    def listmounts(volume: str) -> list[str]: ...
    def listvolumes() -> list[str]: ...

if sys.version_info >= (3, 10) and sys.platform == "linux":
    EFD_CLOEXEC: int
    EFD_NONBLOCK: int
    EFD_SEMAPHORE: int
    SPLICE_F_MORE: int
    SPLICE_F_MOVE: int
    SPLICE_F_NONBLOCK: int
    def eventfd(initval: int, flags: int = 524288) -> FileDescriptor: ...
    def eventfd_read(fd: FileDescriptor) -> int: ...
    def eventfd_write(fd: FileDescriptor, value: int) -> None: ...
    def splice(
        src: FileDescriptor,
        dst: FileDescriptor,
        count: int,
        offset_src: int | None = ...,
        offset_dst: int | None = ...,
        flags: int = 0,
    ) -> int: ...

if sys.version_info >= (3, 12) and sys.platform == "linux":
    CLONE_FILES: int
    CLONE_FS: int
    CLONE_NEWCGROUP: int  # Linux 4.6+
    CLONE_NEWIPC: int  # Linux 2.6.19+
    CLONE_NEWNET: int  # Linux 2.6.24+
    CLONE_NEWNS: int
    CLONE_NEWPID: int  # Linux 3.8+
    CLONE_NEWTIME: int  # Linux 5.6+
    CLONE_NEWUSER: int  # Linux 3.8+
    CLONE_NEWUTS: int  # Linux 2.6.19+
    CLONE_SIGHAND: int
    CLONE_SYSVSEM: int  # Linux 2.6.26+
    CLONE_THREAD: int
    CLONE_VM: int
    def unshare(flags: int) -> None: ...
    def setns(fd: FileDescriptorLike, nstype: int = 0) -> None: ...

if sys.version_info >= (3, 13) and sys.platform != "win32":
    def posix_openpt(oflag: int, /) -> int: ...
    def grantpt(fd: FileDescriptorLike, /) -> None: ...
    def unlockpt(fd: FileDescriptorLike, /) -> None: ...
    def ptsname(fd: FileDescriptorLike, /) -> str: ...

if sys.version_info >= (3, 13) and sys.platform == "linux":
    TFD_TIMER_ABSTIME: Final = 1
    TFD_TIMER_CANCEL_ON_SET: Final = 2
    TFD_NONBLOCK: Final[int]
    TFD_CLOEXEC: Final[int]
    POSIX_SPAWN_CLOSEFROM: Final[int]

    def timerfd_create(clockid: int, /, *, flags: int = 0) -> int: ...
    def timerfd_settime(
        fd: FileDescriptor, /, *, flags: int = 0, initial: float = 0.0, interval: float = 0.0
    ) -> tuple[float, float]: ...
    def timerfd_settime_ns(fd: FileDescriptor, /, *, flags: int = 0, initial: int = 0, interval: int = 0) -> tuple[int, int]: ...
    def timerfd_gettime(fd: FileDescriptor, /) -> tuple[float, float]: ...
    def timerfd_gettime_ns(fd: FileDescriptor, /) -> tuple[int, int]: ...

if sys.version_info >= (3, 13) or sys.platform != "win32":
    # Added to Windows in 3.13.
    def fchmod(fd: int, mode: int) -> None: ...

if sys.platform != "linux":
    if sys.version_info >= (3, 13) or sys.platform != "win32":
        # Added to Windows in 3.13.
        def lchmod(path: StrOrBytesPath, mode: int) -> None: ...
