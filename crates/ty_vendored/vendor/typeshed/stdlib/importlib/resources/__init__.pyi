"""
Read resources contained within a package.

This codebase is shared between importlib.resources in the stdlib
and importlib_resources in PyPI. See
https://github.com/python/importlib_metadata/wiki/Development-Methodology
for more detail.
"""

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
    def open_binary(package: Package, resource: Resource) -> BinaryIO:
        """Return a file-like object opened for binary reading of the resource."""

    def open_text(package: Package, resource: Resource, encoding: str = "utf-8", errors: str = "strict") -> TextIO:
        """Return a file-like object opened for text reading of the resource."""

    def read_binary(package: Package, resource: Resource) -> bytes:
        """Return the binary contents of the resource."""

    def read_text(package: Package, resource: Resource, encoding: str = "utf-8", errors: str = "strict") -> str:
        """Return the decoded string of the resource.

        The decoding-related arguments have the same semantics as those of
        bytes.decode().
        """

    def path(package: Package, resource: Resource) -> AbstractContextManager[Path, Literal[False]]:
        """A context manager providing a file path object to the resource.

        If the resource does not already exist on its own on the file system,
        a temporary file will be created. If the file was created, the file
        will be deleted upon exiting the context manager (no exception is
        raised if the file was deleted prior to the context manager
        exiting).
        """

    def is_resource(package: Package, name: str) -> bool:
        """True if `name` is a resource inside `package`.

        Directories are *not* resources.
        """
    if sys.version_info >= (3, 11):
        @deprecated("Deprecated since Python 3.11. Use `files(anchor).iterdir()`.")
        def contents(package: Package) -> Iterator[str]:
            """Return an iterable of entries in `package`.

            Note that not all entries are resources.  Specifically, directories are
            not considered resources.  Use `is_resource()` on each entry returned here
            to check if it is a resource or not.
            """
    else:
        def contents(package: Package) -> Iterator[str]:
            """Return an iterable of entries in 'package'.

            Note that not all entries are resources.  Specifically, directories are
            not considered resources.  Use `is_resource()` on each entry returned here
            to check if it is a resource or not.
            """

if sys.version_info >= (3, 11):
    from importlib.resources._common import as_file as as_file
else:
    def as_file(path: Traversable) -> AbstractContextManager[Path, Literal[False]]:
        """
        Given a Traversable object, return that object as a
        path on the local file system in a context manager.
        """

if sys.version_info >= (3, 11):
    from importlib.resources._common import files as files
else:
    def files(package: Package) -> Traversable:
        """
        Get a Traversable resource from a package
        """

if sys.version_info >= (3, 11):
    from importlib.resources.abc import ResourceReader as ResourceReader
elif sys.version_info >= (3, 10):
    from importlib.abc import ResourceReader as ResourceReader
