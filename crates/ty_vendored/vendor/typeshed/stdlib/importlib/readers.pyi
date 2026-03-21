"""
Compatibility shim for .resources.readers as found on Python 3.10.

Consumers that can rely on Python 3.11 should use the other
module directly.
"""

# On py311+, things are actually defined in importlib.resources.readers,
# and re-exported here,
# but doing it this way leads to less code duplication for us

import pathlib
import sys
import zipfile
from _typeshed import StrPath
from collections.abc import Iterable, Iterator
from io import BufferedReader
from typing import Literal, NoReturn, TypeVar
from typing_extensions import Never

if sys.version_info >= (3, 10):
    from importlib._bootstrap_external import FileLoader
    from zipimport import zipimporter

if sys.version_info >= (3, 11):
    from importlib.resources import abc
else:
    from importlib import abc

if sys.version_info >= (3, 10):
    if sys.version_info >= (3, 11):
        __all__ = ["FileReader", "ZipReader", "MultiplexedPath", "NamespaceReader"]

    if sys.version_info < (3, 11):
        _T = TypeVar("_T")

        def remove_duplicates(items: Iterable[_T]) -> Iterator[_T]: ...

    class FileReader(abc.TraversableResources):
        path: pathlib.Path
        def __init__(self, loader: FileLoader) -> None: ...
        def resource_path(self, resource: StrPath) -> str:
            """
            Return the file system path to prevent
            `resources.path()` from creating a temporary
            copy.
            """

        def files(self) -> pathlib.Path: ...

    class ZipReader(abc.TraversableResources):
        prefix: str
        archive: str
        def __init__(self, loader: zipimporter, module: str) -> None: ...
        def open_resource(self, resource: str) -> BufferedReader: ...
        def is_resource(self, path: StrPath) -> bool:
            """
            Workaround for `zipfile.Path.is_file` returning true
            for non-existent paths.
            """

        def files(self) -> zipfile.Path: ...

    class MultiplexedPath(abc.Traversable):
        """
        Given a series of Traversable objects, implement a merged
        version of the interface across all objects. Useful for
        namespace packages which may be multihomed at a single
        name.
        """

        def __init__(self, *paths: abc.Traversable) -> None: ...
        def iterdir(self) -> Iterator[abc.Traversable]: ...
        def read_bytes(self) -> NoReturn: ...
        def read_text(self, *args: Never, **kwargs: Never) -> NoReturn: ...  # type: ignore[override]
        def is_dir(self) -> Literal[True]: ...
        def is_file(self) -> Literal[False]: ...

        if sys.version_info >= (3, 12):
            def joinpath(self, *descendants: StrPath) -> abc.Traversable: ...
        elif sys.version_info >= (3, 11):
            def joinpath(self, child: StrPath) -> abc.Traversable: ...  # type: ignore[override]
        else:
            def joinpath(self, child: str) -> abc.Traversable: ...

        if sys.version_info < (3, 12):
            __truediv__ = joinpath

        def open(self, *args: Never, **kwargs: Never) -> NoReturn: ...  # type: ignore[override]
        @property
        def name(self) -> str: ...

    class NamespaceReader(abc.TraversableResources):
        path: MultiplexedPath
        def __init__(self, namespace_path: Iterable[str]) -> None: ...
        def resource_path(self, resource: str) -> str:
            """
            Return the file system path to prevent
            `resources.path()` from creating a temporary
            copy.
            """

        def files(self) -> MultiplexedPath: ...
