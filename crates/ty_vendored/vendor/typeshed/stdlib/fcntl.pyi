"""This module performs file control and I/O control on file
descriptors.  It is an interface to the fcntl() and ioctl() Unix
routines.  File descriptors can be obtained with the fileno() method of
a file or socket object.
"""

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
    def fcntl(fd: FileDescriptorLike, cmd: int, arg: int = 0, /) -> int:
        """Perform the operation `cmd` on file descriptor fd.

        The values used for `cmd` are operating system dependent, and are available
        as constants in the fcntl module, using the same names as used in
        the relevant C header files.  The argument arg is optional, and
        defaults to 0; it may be an int or a string.  If arg is given as a string,
        the return value of fcntl is a string of that length, containing the
        resulting value put in the arg buffer by the operating system.  The length
        of the arg string is not allowed to exceed 1024 bytes.  If the arg given
        is an integer or if none is specified, the result value is an integer
        corresponding to the return value of the fcntl call in the C code.
        """

    @overload
    def fcntl(fd: FileDescriptorLike, cmd: int, arg: str | ReadOnlyBuffer, /) -> bytes: ...
    # If arg is an int, return int
    @overload
    def ioctl(fd: FileDescriptorLike, request: int, arg: int = 0, mutate_flag: bool = True, /) -> int:
        """Perform the operation `request` on file descriptor `fd`.

        The values used for `request` are operating system dependent, and are available
        as constants in the fcntl or termios library modules, using the same names as
        used in the relevant C header files.

        The argument `arg` is optional, and defaults to 0; it may be an int or a
        buffer containing character data (most likely a string or an array).

        If the argument is a mutable buffer (such as an array) and if the
        mutate_flag argument (which is only allowed in this case) is true then the
        buffer is (in effect) passed to the operating system and changes made by
        the OS will be reflected in the contents of the buffer after the call has
        returned.  The return value is the integer returned by the ioctl system
        call.

        If the argument is a mutable buffer and the mutable_flag argument is false,
        the behavior is as if a string had been passed.

        If the argument is an immutable buffer (most likely a string) then a copy
        of the buffer is passed to the operating system and the return value is a
        string of the same length containing whatever the operating system put in
        the buffer.  The length of the arg buffer in this case is not allowed to
        exceed 1024 bytes.

        If the arg given is an integer or if none is specified, the result value is
        an integer corresponding to the return value of the ioctl call in the C
        code.
        """
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
    def flock(fd: FileDescriptorLike, operation: int, /) -> None:
        """Perform the lock operation `operation` on file descriptor `fd`.

        See the Unix manual page for flock(2) for details (On some systems, this
        function is emulated using fcntl()).
        """

    def lockf(fd: FileDescriptorLike, cmd: int, len: int = 0, start: int = 0, whence: int = 0, /) -> Any:
        """A wrapper around the fcntl() locking calls.

        `fd` is the file descriptor of the file to lock or unlock, and operation is one
        of the following values:

            LOCK_UN - unlock
            LOCK_SH - acquire a shared lock
            LOCK_EX - acquire an exclusive lock

        When operation is LOCK_SH or LOCK_EX, it can also be bitwise ORed with
        LOCK_NB to avoid blocking on lock acquisition.  If LOCK_NB is used and the
        lock cannot be acquired, an OSError will be raised and the exception will
        have an errno attribute set to EACCES or EAGAIN (depending on the operating
        system -- for portability, check for either value).

        `len` is the number of bytes to lock, with the default meaning to lock to
        EOF.  `start` is the byte offset, relative to `whence`, to that the lock
        starts.  `whence` is as with fileobj.seek(), specifically:

            0 - relative to the start of the file (SEEK_SET)
            1 - relative to the current buffer position (SEEK_CUR)
            2 - relative to the end of the file (SEEK_END)
        """
