import sys
from typing import Final

SF_APPEND: Final = 0x00040000
SF_ARCHIVED: Final = 0x00010000
SF_IMMUTABLE: Final = 0x00020000
SF_NOUNLINK: Final = 0x00100000
SF_SNAPSHOT: Final = 0x00200000

ST_MODE: Final = 0
ST_INO: Final = 1
ST_DEV: Final = 2
ST_NLINK: Final = 3
ST_UID: Final = 4
ST_GID: Final = 5
ST_SIZE: Final = 6
ST_ATIME: Final = 7
ST_MTIME: Final = 8
ST_CTIME: Final = 9

S_IFIFO: Final = 0o010000
S_IFLNK: Final = 0o120000
S_IFREG: Final = 0o100000
S_IFSOCK: Final = 0o140000
S_IFBLK: Final = 0o060000
S_IFCHR: Final = 0o020000
S_IFDIR: Final = 0o040000

# These are 0 on systems that don't support the specific kind of file.
# Example: Linux doesn't support door files, so S_IFDOOR is 0 on linux.
S_IFDOOR: int
S_IFPORT: int
S_IFWHT: int

S_ISUID: Final = 0o4000
S_ISGID: Final = 0o2000
S_ISVTX: Final = 0o1000

S_IRWXU: Final = 0o0700
S_IRUSR: Final = 0o0400
S_IWUSR: Final = 0o0200
S_IXUSR: Final = 0o0100

S_IRWXG: Final = 0o0070
S_IRGRP: Final = 0o0040
S_IWGRP: Final = 0o0020
S_IXGRP: Final = 0o0010

S_IRWXO: Final = 0o0007
S_IROTH: Final = 0o0004
S_IWOTH: Final = 0o0002
S_IXOTH: Final = 0o0001

S_ENFMT: Final = 0o2000
S_IREAD: Final = 0o0400
S_IWRITE: Final = 0o0200
S_IEXEC: Final = 0o0100

UF_APPEND: Final = 0x00000004
UF_COMPRESSED: Final = 0x00000020  # OS X 10.6+ only
UF_HIDDEN: Final = 0x00008000  # OX X 10.5+ only
UF_IMMUTABLE: Final = 0x00000002
UF_NODUMP: Final = 0x00000001
UF_NOUNLINK: Final = 0x00000010
UF_OPAQUE: Final = 0x00000008

def S_IMODE(mode: int, /) -> int: ...
def S_IFMT(mode: int, /) -> int: ...
def S_ISBLK(mode: int, /) -> bool: ...
def S_ISCHR(mode: int, /) -> bool: ...
def S_ISDIR(mode: int, /) -> bool: ...
def S_ISDOOR(mode: int, /) -> bool: ...
def S_ISFIFO(mode: int, /) -> bool: ...
def S_ISLNK(mode: int, /) -> bool: ...
def S_ISPORT(mode: int, /) -> bool: ...
def S_ISREG(mode: int, /) -> bool: ...
def S_ISSOCK(mode: int, /) -> bool: ...
def S_ISWHT(mode: int, /) -> bool: ...
def filemode(mode: int, /) -> str: ...

if sys.platform == "win32":
    IO_REPARSE_TAG_SYMLINK: int
    IO_REPARSE_TAG_MOUNT_POINT: int
    IO_REPARSE_TAG_APPEXECLINK: int

if sys.platform == "win32":
    FILE_ATTRIBUTE_ARCHIVE: Final = 32
    FILE_ATTRIBUTE_COMPRESSED: Final = 2048
    FILE_ATTRIBUTE_DEVICE: Final = 64
    FILE_ATTRIBUTE_DIRECTORY: Final = 16
    FILE_ATTRIBUTE_ENCRYPTED: Final = 16384
    FILE_ATTRIBUTE_HIDDEN: Final = 2
    FILE_ATTRIBUTE_INTEGRITY_STREAM: Final = 32768
    FILE_ATTRIBUTE_NORMAL: Final = 128
    FILE_ATTRIBUTE_NOT_CONTENT_INDEXED: Final = 8192
    FILE_ATTRIBUTE_NO_SCRUB_DATA: Final = 131072
    FILE_ATTRIBUTE_OFFLINE: Final = 4096
    FILE_ATTRIBUTE_READONLY: Final = 1
    FILE_ATTRIBUTE_REPARSE_POINT: Final = 1024
    FILE_ATTRIBUTE_SPARSE_FILE: Final = 512
    FILE_ATTRIBUTE_SYSTEM: Final = 4
    FILE_ATTRIBUTE_TEMPORARY: Final = 256
    FILE_ATTRIBUTE_VIRTUAL: Final = 65536

if sys.version_info >= (3, 13):
    # Varies by platform.
    SF_SETTABLE: Final[int]
    # https://github.com/python/cpython/issues/114081#issuecomment-2119017790
    # SF_RESTRICTED: Literal[0x00080000]
    SF_FIRMLINK: Final = 0x00800000
    SF_DATALESS: Final = 0x40000000

    if sys.platform == "darwin":
        SF_SUPPORTED: Final = 0x9F0000
        SF_SYNTHETIC: Final = 0xC0000000

    UF_TRACKED: Final = 0x00000040
    UF_DATAVAULT: Final = 0x00000080
    UF_SETTABLE: Final = 0x0000FFFF
