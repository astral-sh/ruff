"""
Interface adapters for low-level readers.
"""

import abc
import sys
from _typeshed import StrPath
from collections.abc import Iterator
from io import TextIOWrapper
from typing import IO, Any, BinaryIO, Literal, NoReturn, overload
from typing_extensions import Never

if sys.version_info >= (3, 11):
    from .abc import Traversable, TraversableResources

    class SimpleReader(abc.ABC):
        """
        The minimum, low-level interface required from a resource
        provider.
        """

        @property
        @abc.abstractmethod
        def package(self) -> str:
            """
            The name of the package for which this reader loads resources.
            """

        @abc.abstractmethod
        def children(self) -> list[SimpleReader]:
            """
            Obtain an iterable of SimpleReader for available
            child containers (e.g. directories).
            """

        @abc.abstractmethod
        def resources(self) -> list[str]:
            """
            Obtain available named resources for this virtual package.
            """

        @abc.abstractmethod
        def open_binary(self, resource: str) -> BinaryIO:
            """
            Obtain a File-like for a named resource.
            """

        @property
        def name(self) -> str: ...

    class ResourceHandle(Traversable, metaclass=abc.ABCMeta):
        """
        Handle to a named resource in a ResourceReader.
        """

        parent: ResourceContainer
        def __init__(self, parent: ResourceContainer, name: str) -> None: ...
        def is_file(self) -> Literal[True]: ...
        def is_dir(self) -> Literal[False]: ...
        @overload
        def open(
            self,
            mode: Literal["r"] = "r",
            encoding: str | None = None,
            errors: str | None = None,
            newline: str | None = None,
            line_buffering: bool = False,
            write_through: bool = False,
        ) -> TextIOWrapper: ...
        @overload
        def open(self, mode: Literal["rb"]) -> BinaryIO: ...
        @overload
        def open(self, mode: str) -> IO[Any]: ...
        def joinpath(self, name: Never) -> NoReturn: ...  # type: ignore[override]

    class ResourceContainer(Traversable, metaclass=abc.ABCMeta):
        """
        Traversable container for a package's resources via its reader.
        """

        reader: SimpleReader
        def __init__(self, reader: SimpleReader) -> None: ...
        def is_dir(self) -> Literal[True]: ...
        def is_file(self) -> Literal[False]: ...
        def iterdir(self) -> Iterator[ResourceHandle | ResourceContainer]: ...
        def open(self, *args: Never, **kwargs: Never) -> NoReturn: ...  # type: ignore[override]
        if sys.version_info < (3, 12):
            def joinpath(self, *descendants: StrPath) -> Traversable: ...

    class TraversableReader(TraversableResources, SimpleReader, metaclass=abc.ABCMeta):
        """
        A TraversableResources based on SimpleReader. Resource providers
        may derive from this class to provide the TraversableResources
        interface by supplying the SimpleReader interface.
        """

        def files(self) -> ResourceContainer: ...
