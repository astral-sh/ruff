import os
import sys
from collections.abc import Iterator
from contextlib import AbstractContextManager
from pathlib import Path
from types import ModuleType
from typing import Any, BinaryIO, Literal, TextIO
from typing_extensions import TypeAlias, deprecated

if sys.version_info >= (3, 11):
    from importlib.resources.abc import Traversable
else:
    from importlib.abc import Traversable

if sys.version_info >= (3, 11):
    from importlib.resources._common import Package as Package
else:
    Package: TypeAlias = str | ModuleType

__all__ = [
    "Package",
    "as_file",
    "contents",
    "files",
    "is_resource",
    "open_binary",
    "open_text",
    "path",
    "read_binary",
    "read_text",
]

if sys.version_info >= (3, 10):
    __all__ += ["ResourceReader"]

if sys.version_info < (3, 13):
    __all__ += ["Resource"]

if sys.version_info < (3, 11):
    Resource: TypeAlias = str | os.PathLike[Any]
elif sys.version_info < (3, 13):
    Resource: TypeAlias = str

if sys.version_info >= (3, 12):
    from importlib.resources._common import Anchor as Anchor

    __all__ += ["Anchor"]

if sys.version_info >= (3, 13):
    from importlib.resources._functional import (
        contents as contents,
        is_resource as is_resource,
        open_binary as open_binary,
        open_text as open_text,
        path as path,
        read_binary as read_binary,
        read_text as read_text,
    )

else:
    def open_binary(package: Package, resource: Resource) -> BinaryIO: ...
    def open_text(package: Package, resource: Resource, encoding: str = "utf-8", errors: str = "strict") -> TextIO: ...
    def read_binary(package: Package, resource: Resource) -> bytes: ...
    def read_text(package: Package, resource: Resource, encoding: str = "utf-8", errors: str = "strict") -> str: ...
    def path(package: Package, resource: Resource) -> AbstractContextManager[Path, Literal[False]]: ...
    def is_resource(package: Package, name: str) -> bool: ...
    if sys.version_info >= (3, 11):
        @deprecated("Deprecated since Python 3.11. Use `files(anchor).iterdir()`.")
        def contents(package: Package) -> Iterator[str]: ...
    else:
        def contents(package: Package) -> Iterator[str]: ...

if sys.version_info >= (3, 11):
    from importlib.resources._common import as_file as as_file
else:
    def as_file(path: Traversable) -> AbstractContextManager[Path, Literal[False]]: ...

if sys.version_info >= (3, 11):
    from importlib.resources._common import files as files
else:
    def files(package: Package) -> Traversable: ...

if sys.version_info >= (3, 11):
    from importlib.resources.abc import ResourceReader as ResourceReader
elif sys.version_info >= (3, 10):
    from importlib.abc import ResourceReader as ResourceReader
