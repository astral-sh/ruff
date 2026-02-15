import sys
from _typeshed import FileDescriptorLike, ReadOnlyBuffer, WriteableBuffer
from typing import Any, Final, Literal, overload
from typing_extensions import Buffer

if sys.platform != "win32":
    FASYNC: Final[int]
    FD_CLOEXEC: Final[int]
    F_DUPFD: Final[int]
    F_DUPFD_CLOEXEC: Final[int]
    F_GETFD: Final[int]
    F_GETFL: Final[int]
    F_GETLK: Final[int]
    F_GETOWN: Final[int]
    F_RDLCK: Final[int]
    F_SETFD: Final[int]
    F_SETFL: Final[int]
    F_SETLK: Final[int]
    F_SETLKW: Final[int]
    F_SETOWN: Final[int]
    F_UNLCK: Final[int]
    F_WRLCK: Final[int]

    F_GETLEASE: Final[int]
    F_SETLEASE: Final[int]
    if sys.platform == "darwin":
        F_FULLFSYNC: Final[int]
        F_NOCACHE: Final[int]
        F_GETPATH: Final[int]
    if sys.platform == "linux":
        F_SETLKW64: Final[int]
        F_SETSIG: Final[int]
        F_SHLCK: Final[int]
        F_SETLK64: Final[int]
        F_GETSIG: Final[int]
        F_NOTIFY: Final[int]
        F_EXLCK: Final[int]
        F_GETLK64: Final[int]
        F_ADD_SEALS: Final[int]
        F_GET_SEALS: Final[int]
        F_SEAL_GROW: Final[int]
        F_SEAL_SEAL: Final[int]
        F_SEAL_SHRINK: Final[int]
        F_SEAL_WRITE: Final[int]
        F_OFD_GETLK: Final[int]
        F_OFD_SETLK: Final[int]
        F_OFD_SETLKW: Final[int]

        if sys.version_info >= (3, 10):
            F_GETPIPE_SZ: Final[int]
            F_SETPIPE_SZ: Final[int]

        DN_ACCESS: Final[int]
        DN_ATTRIB: Final[int]
        DN_CREATE: Final[int]
        DN_DELETE: Final[int]
        DN_MODIFY: Final[int]
        DN_MULTISHOT: Final[int]
        DN_RENAME: Final[int]

    LOCK_EX: Final[int]
    LOCK_NB: Final[int]
    LOCK_SH: Final[int]
    LOCK_UN: Final[int]
    if sys.platform == "linux":
        LOCK_MAND: Final[int]
        LOCK_READ: Final[int]
        LOCK_RW: Final[int]
        LOCK_WRITE: Final[int]

    if sys.platform == "linux":
        # Constants for the POSIX STREAMS interface. Present in glibc until 2.29 (released February 2019).
        # Never implemented on BSD, and considered "obsolescent" starting in POSIX 2008.
        # Probably still used on Solaris.
        I_ATMARK: Final[int]
        I_CANPUT: Final[int]
        I_CKBAND: Final[int]
        I_FDINSERT: Final[int]
        I_FIND: Final[int]
        I_FLUSH: Final[int]
        I_FLUSHBAND: Final[int]
        I_GETBAND: Final[int]
        I_GETCLTIME: Final[int]
        I_GETSIG: Final[int]
        I_GRDOPT: Final[int]
        I_GWROPT: Final[int]
        I_LINK: Final[int]
        I_LIST: Final[int]
        I_LOOK: Final[int]
        I_NREAD: Final[int]
        I_PEEK: Final[int]
        I_PLINK: Final[int]
        I_POP: Final[int]
        I_PUNLINK: Final[int]
        I_PUSH: Final[int]
        I_RECVFD: Final[int]
        I_SENDFD: Final[int]
        I_SETCLTIME: Final[int]
        I_SETSIG: Final[int]
        I_SRDOPT: Final[int]
        I_STR: Final[int]
        I_SWROPT: Final[int]
        I_UNLINK: Final[int]

    if sys.version_info >= (3, 12) and sys.platform == "linux":
        FICLONE: Final[int]
        FICLONERANGE: Final[int]

    if sys.version_info >= (3, 13) and sys.platform == "linux":
        F_OWNER_TID: Final = 0
        F_OWNER_PID: Final = 1
        F_OWNER_PGRP: Final = 2
        F_SETOWN_EX: Final = 15
        F_GETOWN_EX: Final = 16
        F_SEAL_FUTURE_WRITE: Final = 16
        F_GET_RW_HINT: Final = 1035
        F_SET_RW_HINT: Final = 1036
        F_GET_FILE_RW_HINT: Final = 1037
        F_SET_FILE_RW_HINT: Final = 1038
        RWH_WRITE_LIFE_NOT_SET: Final = 0
        RWH_WRITE_LIFE_NONE: Final = 1
        RWH_WRITE_LIFE_SHORT: Final = 2
        RWH_WRITE_LIFE_MEDIUM: Final = 3
        RWH_WRITE_LIFE_LONG: Final = 4
        RWH_WRITE_LIFE_EXTREME: Final = 5

    if sys.version_info >= (3, 11) and sys.platform == "darwin":
        F_OFD_SETLK: Final = 90
        F_OFD_SETLKW: Final = 91
        F_OFD_GETLK: Final = 92

    if sys.version_info >= (3, 13) and sys.platform != "linux":
        # OSx and NetBSD
        F_GETNOSIGPIPE: Final[int]
        F_SETNOSIGPIPE: Final[int]
        # OSx and FreeBSD
        F_RDAHEAD: Final[int]

    @overload
    def fcntl(fd: FileDescriptorLike, cmd: int, arg: int = 0, /) -> int: ...
    @overload
    def fcntl(fd: FileDescriptorLike, cmd: int, arg: str | ReadOnlyBuffer, /) -> bytes: ...
    # If arg is an int, return int
    @overload
    def ioctl(fd: FileDescriptorLike, request: int, arg: int = 0, mutate_flag: bool = True, /) -> int: ...
    # The return type works as follows:
    # - If arg is a read-write buffer, return int if mutate_flag is True, otherwise bytes
    # - If arg is a read-only buffer, return bytes (and ignore the value of mutate_flag)
    # We can't represent that precisely as we can't distinguish between read-write and read-only
    # buffers, so we add overloads for a few unambiguous cases and use Any for the rest.
    @overload
    def ioctl(fd: FileDescriptorLike, request: int, arg: bytes, mutate_flag: bool = True, /) -> bytes: ...
    @overload
    def ioctl(fd: FileDescriptorLike, request: int, arg: WriteableBuffer, mutate_flag: Literal[False], /) -> bytes: ...
    @overload
    def ioctl(fd: FileDescriptorLike, request: int, arg: Buffer, mutate_flag: bool = True, /) -> Any: ...
    def flock(fd: FileDescriptorLike, operation: int, /) -> None: ...
    def lockf(fd: FileDescriptorLike, cmd: int, len: int = 0, start: int = 0, whence: int = 0, /) -> Any: ...
