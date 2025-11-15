import sys
from _stat import (
    S_ENFMT as S_ENFMT,
    S_IEXEC as S_IEXEC,
    S_IFBLK as S_IFBLK,
    S_IFCHR as S_IFCHR,
    S_IFDIR as S_IFDIR,
    S_IFDOOR as S_IFDOOR,
    S_IFIFO as S_IFIFO,
    S_IFLNK as S_IFLNK,
    S_IFMT as S_IFMT,
    S_IFPORT as S_IFPORT,
    S_IFREG as S_IFREG,
    S_IFSOCK as S_IFSOCK,
    S_IFWHT as S_IFWHT,
    S_IMODE as S_IMODE,
    S_IREAD as S_IREAD,
    S_IRGRP as S_IRGRP,
    S_IROTH as S_IROTH,
    S_IRUSR as S_IRUSR,
    S_IRWXG as S_IRWXG,
    S_IRWXO as S_IRWXO,
    S_IRWXU as S_IRWXU,
    S_ISBLK as S_ISBLK,
    S_ISCHR as S_ISCHR,
    S_ISDIR as S_ISDIR,
    S_ISDOOR as S_ISDOOR,
    S_ISFIFO as S_ISFIFO,
    S_ISGID as S_ISGID,
    S_ISLNK as S_ISLNK,
    S_ISPORT as S_ISPORT,
    S_ISREG as S_ISREG,
    S_ISSOCK as S_ISSOCK,
    S_ISUID as S_ISUID,
    S_ISVTX as S_ISVTX,
    S_ISWHT as S_ISWHT,
    S_IWGRP as S_IWGRP,
    S_IWOTH as S_IWOTH,
    S_IWRITE as S_IWRITE,
    S_IWUSR as S_IWUSR,
    S_IXGRP as S_IXGRP,
    S_IXOTH as S_IXOTH,
    S_IXUSR as S_IXUSR,
    SF_APPEND as SF_APPEND,
    SF_ARCHIVED as SF_ARCHIVED,
    SF_IMMUTABLE as SF_IMMUTABLE,
    SF_NOUNLINK as SF_NOUNLINK,
    SF_SNAPSHOT as SF_SNAPSHOT,
    ST_ATIME as ST_ATIME,
    ST_CTIME as ST_CTIME,
    ST_DEV as ST_DEV,
    ST_GID as ST_GID,
    ST_INO as ST_INO,
    ST_MODE as ST_MODE,
    ST_MTIME as ST_MTIME,
    ST_NLINK as ST_NLINK,
    ST_SIZE as ST_SIZE,
    ST_UID as ST_UID,
    UF_APPEND as UF_APPEND,
    UF_COMPRESSED as UF_COMPRESSED,
    UF_HIDDEN as UF_HIDDEN,
    UF_IMMUTABLE as UF_IMMUTABLE,
    UF_NODUMP as UF_NODUMP,
    UF_NOUNLINK as UF_NOUNLINK,
    UF_OPAQUE as UF_OPAQUE,
    filemode as filemode,
)
from typing import Final

if sys.platform == "win32":
    from _stat import (
        IO_REPARSE_TAG_APPEXECLINK as IO_REPARSE_TAG_APPEXECLINK,
        IO_REPARSE_TAG_MOUNT_POINT as IO_REPARSE_TAG_MOUNT_POINT,
        IO_REPARSE_TAG_SYMLINK as IO_REPARSE_TAG_SYMLINK,
    )

if sys.version_info >= (3, 13):
    from _stat import (
        SF_DATALESS as SF_DATALESS,
        SF_FIRMLINK as SF_FIRMLINK,
        SF_SETTABLE as SF_SETTABLE,
        UF_DATAVAULT as UF_DATAVAULT,
        UF_SETTABLE as UF_SETTABLE,
        UF_TRACKED as UF_TRACKED,
    )

    if sys.platform == "darwin":
        from _stat import SF_SUPPORTED as SF_SUPPORTED, SF_SYNTHETIC as SF_SYNTHETIC

# _stat.c defines FILE_ATTRIBUTE_* constants conditionally,
# making them available only at runtime on Windows.
# stat.py unconditionally redefines the same FILE_ATTRIBUTE_* constants
# on all platforms.
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
    # https://github.com/python/cpython/issues/114081#issuecomment-2119017790
    SF_RESTRICTED: Final = 0x00080000
