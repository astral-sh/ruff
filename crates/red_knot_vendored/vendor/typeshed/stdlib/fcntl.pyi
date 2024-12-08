import sys
from _typeshed import FileDescriptorLike, ReadOnlyBuffer, WriteableBuffer
from typing import Any, Final, Literal, overload
from typing_extensions import Buffer

if sys.platform != "win32":
    FASYNC: int
    FD_CLOEXEC: int
    F_DUPFD: int
    F_DUPFD_CLOEXEC: int
    F_GETFD: int
    F_GETFL: int
    F_GETLK: int
    F_GETOWN: int
    F_RDLCK: int
    F_SETFD: int
    F_SETFL: int
    F_SETLK: int
    F_SETLKW: int
    F_SETOWN: int
    F_UNLCK: int
    F_WRLCK: int

    F_GETLEASE: int
    F_SETLEASE: int
    if sys.platform == "darwin":
        F_FULLFSYNC: int
        F_NOCACHE: int
        if sys.version_info >= (3, 9):
            F_GETPATH: int
    if sys.platform == "linux":
        F_SETLKW64: int
        F_SETSIG: int
        F_SHLCK: int
        F_SETLK64: int
        F_GETSIG: int
        F_NOTIFY: int
        F_EXLCK: int
        F_GETLK64: int
        F_ADD_SEALS: int
        F_GET_SEALS: int
        F_SEAL_GROW: int
        F_SEAL_SEAL: int
        F_SEAL_SHRINK: int
        F_SEAL_WRITE: int
        if sys.version_info >= (3, 9):
            F_OFD_GETLK: Final[int]
            F_OFD_SETLK: Final[int]
            F_OFD_SETLKW: Final[int]

        if sys.version_info >= (3, 10):
            F_GETPIPE_SZ: int
            F_SETPIPE_SZ: int

        DN_ACCESS: int
        DN_ATTRIB: int
        DN_CREATE: int
        DN_DELETE: int
        DN_MODIFY: int
        DN_MULTISHOT: int
        DN_RENAME: int

    LOCK_EX: int
    LOCK_NB: int
    LOCK_SH: int
    LOCK_UN: int
    if sys.platform == "linux":
        LOCK_MAND: int
        LOCK_READ: int
        LOCK_RW: int
        LOCK_WRITE: int

    if sys.platform == "linux":
        # Constants for the POSIX STREAMS interface. Present in glibc until 2.29 (released February 2019).
        # Never implemented on BSD, and considered "obsolescent" starting in POSIX 2008.
        # Probably still used on Solaris.
        I_ATMARK: int
        I_CANPUT: int
        I_CKBAND: int
        I_FDINSERT: int
        I_FIND: int
        I_FLUSH: int
        I_FLUSHBAND: int
        I_GETBAND: int
        I_GETCLTIME: int
        I_GETSIG: int
        I_GRDOPT: int
        I_GWROPT: int
        I_LINK: int
        I_LIST: int
        I_LOOK: int
        I_NREAD: int
        I_PEEK: int
        I_PLINK: int
        I_POP: int
        I_PUNLINK: int
        I_PUSH: int
        I_RECVFD: int
        I_SENDFD: int
        I_SETCLTIME: int
        I_SETSIG: int
        I_SRDOPT: int
        I_STR: int
        I_SWROPT: int
        I_UNLINK: int

    if sys.version_info >= (3, 12) and sys.platform == "linux":
        FICLONE: int
        FICLONERANGE: int

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
