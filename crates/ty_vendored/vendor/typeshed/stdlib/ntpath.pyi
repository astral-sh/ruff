import sys
from _typeshed import BytesPath, StrOrBytesPath, StrPath
from genericpath import (
    ALLOW_MISSING as ALLOW_MISSING,
    _AllowMissingType,
    commonprefix as commonprefix,
    exists as exists,
    getatime as getatime,
    getctime as getctime,
    getmtime as getmtime,
    getsize as getsize,
    isdir as isdir,
    isfile as isfile,
    samefile as samefile,
    sameopenfile as sameopenfile,
    samestat as samestat,
)
from os import PathLike

# Re-export common definitions from posixpath to reduce duplication
from posixpath import (
    abspath as abspath,
    basename as basename,
    commonpath as commonpath,
    curdir as curdir,
    defpath as defpath,
    devnull as devnull,
    dirname as dirname,
    expanduser as expanduser,
    expandvars as expandvars,
    extsep as extsep,
    isabs as isabs,
    islink as islink,
    ismount as ismount,
    lexists as lexists,
    normcase as normcase,
    normpath as normpath,
    pardir as pardir,
    pathsep as pathsep,
    relpath as relpath,
    sep as sep,
    split as split,
    splitdrive as splitdrive,
    splitext as splitext,
    supports_unicode_filenames as supports_unicode_filenames,
)
from typing import AnyStr, overload
from typing_extensions import LiteralString

if sys.version_info >= (3, 12):
    from posixpath import isjunction as isjunction, splitroot as splitroot
if sys.version_info >= (3, 13):
    from genericpath import isdevdrive as isdevdrive

__all__ = [
    "normcase",
    "isabs",
    "join",
    "splitdrive",
    "split",
    "splitext",
    "basename",
    "dirname",
    "commonprefix",
    "getsize",
    "getmtime",
    "getatime",
    "getctime",
    "islink",
    "exists",
    "lexists",
    "isdir",
    "isfile",
    "ismount",
    "expanduser",
    "expandvars",
    "normpath",
    "abspath",
    "curdir",
    "pardir",
    "sep",
    "pathsep",
    "defpath",
    "altsep",
    "extsep",
    "devnull",
    "realpath",
    "supports_unicode_filenames",
    "relpath",
    "samefile",
    "sameopenfile",
    "samestat",
    "commonpath",
    "ALLOW_MISSING",
]
if sys.version_info >= (3, 12):
    __all__ += ["isjunction", "splitroot"]
if sys.version_info >= (3, 13):
    __all__ += ["isdevdrive", "isreserved"]

altsep: LiteralString

# First parameter is not actually pos-only,
# but must be defined as pos-only in the stub or cross-platform code doesn't type-check,
# as the parameter name is different in posixpath.join()
@overload
def join(path: LiteralString, /, *paths: LiteralString) -> LiteralString: ...
@overload
def join(path: StrPath, /, *paths: StrPath) -> str: ...
@overload
def join(path: BytesPath, /, *paths: BytesPath) -> bytes: ...

if sys.platform == "win32":
    @overload
    def realpath(path: PathLike[AnyStr], *, strict: bool | _AllowMissingType = False) -> AnyStr: ...
    @overload
    def realpath(path: AnyStr, *, strict: bool | _AllowMissingType = False) -> AnyStr: ...

else:
    realpath = abspath

if sys.version_info >= (3, 13):
    def isreserved(path: StrOrBytesPath) -> bool: ...
