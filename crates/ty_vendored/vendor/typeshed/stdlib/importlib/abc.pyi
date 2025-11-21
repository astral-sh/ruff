"""Abstract base classes related to import."""

import _ast
import sys
import types
from _typeshed import ReadableBuffer, StrPath
from abc import ABCMeta, abstractmethod
from collections.abc import Iterator, Mapping, Sequence
from importlib import _bootstrap_external
from importlib.machinery import ModuleSpec
from io import BufferedReader
from typing import IO, Any, Literal, Protocol, overload, runtime_checkable
from typing_extensions import deprecated

if sys.version_info >= (3, 11):
    __all__ = [
        "Loader",
        "MetaPathFinder",
        "PathEntryFinder",
        "ResourceLoader",
        "InspectLoader",
        "ExecutionLoader",
        "FileLoader",
        "SourceLoader",
    ]

    if sys.version_info < (3, 12):
        __all__ += ["Finder", "ResourceReader", "Traversable", "TraversableResources"]

if sys.version_info >= (3, 10):
    from importlib._abc import Loader as Loader
else:
    class Loader(metaclass=ABCMeta):
        """Abstract base class for import loaders."""

        def load_module(self, fullname: str) -> types.ModuleType:
            """Return the loaded module.

            The module must be added to sys.modules and have import-related
            attributes set properly.  The fullname is a str.

            ImportError is raised on failure.

            This method is deprecated in favor of loader.exec_module(). If
            exec_module() exists then it is used to provide a backwards-compatible
            functionality for this method.

            """

        def module_repr(self, module: types.ModuleType) -> str:
            """Return a module's repr.

            Used by the module type when the method does not raise
            NotImplementedError.

            This method is deprecated.

            """

        def create_module(self, spec: ModuleSpec) -> types.ModuleType | None:
            """Return a module to initialize and into which to load.

            This method should raise ImportError if anything prevents it
            from creating a new module.  It may return None to indicate
            that the spec should create the new module.
            """
        # Not defined on the actual class for backwards-compatibility reasons,
        # but expected in new code.
        def exec_module(self, module: types.ModuleType) -> None: ...

if sys.version_info < (3, 12):
    @deprecated("Deprecated since Python 3.3; removed in Python 3.12. Use `MetaPathFinder` or `PathEntryFinder` instead.")
    class Finder(metaclass=ABCMeta):
        """Legacy abstract base class for import finders.

        It may be subclassed for compatibility with legacy third party
        reimplementations of the import system.  Otherwise, finder
        implementations should derive from the more specific MetaPathFinder
        or PathEntryFinder ABCs.

        Deprecated since Python 3.3
        """

@deprecated("Deprecated since Python 3.7. Use `importlib.resources.abc.TraversableResources` instead.")
class ResourceLoader(Loader):
    """Abstract base class for loaders which can return data from their
    back-end storage to facilitate reading data to perform an import.

    This ABC represents one of the optional protocols specified by PEP 302.

    For directly loading resources, use TraversableResources instead. This class
    primarily exists for backwards compatibility with other ABCs in this module.

    """

    @abstractmethod
    def get_data(self, path: str) -> bytes:
        """Abstract method which when implemented should return the bytes for
        the specified path.  The path must be a str.
        """

class InspectLoader(Loader):
    """Abstract base class for loaders which support inspection about the
    modules they can load.

    This ABC represents one of the optional protocols specified by PEP 302.

    """

    def is_package(self, fullname: str) -> bool:
        """Optional method which when implemented should return whether the
        module is a package.  The fullname is a str.  Returns a bool.

        Raises ImportError if the module cannot be found.
        """

    def get_code(self, fullname: str) -> types.CodeType | None:
        """Method which returns the code object for the module.

        The fullname is a str.  Returns a types.CodeType if possible, else
        returns None if a code object does not make sense
        (e.g. built-in module). Raises ImportError if the module cannot be
        found.
        """

    @abstractmethod
    def get_source(self, fullname: str) -> str | None:
        """Abstract method which should return the source code for the
        module.  The fullname is a str.  Returns a str.

        Raises ImportError if the module cannot be found.
        """

    def exec_module(self, module: types.ModuleType) -> None:
        """Execute the module."""

    @staticmethod
    def source_to_code(
        data: ReadableBuffer | str | _ast.Module | _ast.Expression | _ast.Interactive, path: bytes | StrPath = "<string>"
    ) -> types.CodeType:
        """Compile 'data' into a code object.

        The 'data' argument can be anything that compile() can handle. The'path'
        argument should be where the data was retrieved (when applicable).
        """

class ExecutionLoader(InspectLoader):
    """Abstract base class for loaders that wish to support the execution of
    modules as scripts.

    This ABC represents one of the optional protocols specified in PEP 302.

    """

    @abstractmethod
    def get_filename(self, fullname: str) -> str:
        """Abstract method which should return the value that __file__ is to be
        set to.

        Raises ImportError if the module cannot be found.
        """

class SourceLoader(_bootstrap_external.SourceLoader, ResourceLoader, ExecutionLoader, metaclass=ABCMeta):  # type: ignore[misc]  # incompatible definitions of source_to_code in the base classes
    """Abstract base class for loading source code (and optionally any
    corresponding bytecode).

    To support loading from source code, the abstractmethods inherited from
    ResourceLoader and ExecutionLoader need to be implemented. To also support
    loading from bytecode, the optional methods specified directly by this ABC
    is required.

    Inherited abstractmethods not implemented in this ABC:

        * ResourceLoader.get_data
        * ExecutionLoader.get_filename

    """

    @deprecated("Deprecated since Python 3.3. Use `importlib.resources.abc.SourceLoader.path_stats` instead.")
    def path_mtime(self, path: str) -> float:
        """Return the (int) modification time for the path (str)."""

    def set_data(self, path: str, data: bytes) -> None:
        """Write the bytes to the path (if possible).

        Accepts a str path and data as bytes.

        Any needed intermediary directories are to be created. If for some
        reason the file cannot be written because of permissions, fail
        silently.
        """

    def get_source(self, fullname: str) -> str | None:
        """Concrete implementation of InspectLoader.get_source."""

    def path_stats(self, path: str) -> Mapping[str, Any]:
        """Return a metadata dict for the source pointed to by the path (str).
        Possible keys:
        - 'mtime' (mandatory) is the numeric timestamp of last source
          code modification;
        - 'size' (optional) is the size in bytes of the source code.
        """

# The base classes differ starting in 3.10:
if sys.version_info >= (3, 10):
    # Please keep in sync with _typeshed.importlib.MetaPathFinderProtocol
    class MetaPathFinder(metaclass=ABCMeta):
        """Abstract base class for import finders on sys.meta_path."""

        if sys.version_info < (3, 12):
            @deprecated("Deprecated since Python 3.4; removed in Python 3.12. Use `MetaPathFinder.find_spec()` instead.")
            def find_module(self, fullname: str, path: Sequence[str] | None) -> Loader | None:
                """Return a loader for the module.

                If no module is found, return None.  The fullname is a str and
                the path is a list of strings or None.

                This method is deprecated since Python 3.4 in favor of
                finder.find_spec(). If find_spec() exists then backwards-compatible
                functionality is provided for this method.

                """

        def invalidate_caches(self) -> None:
            """An optional method for clearing the finder's cache, if any.
            This method is used by importlib.invalidate_caches().
            """
        # Not defined on the actual class, but expected to exist.
        def find_spec(
            self, fullname: str, path: Sequence[str] | None, target: types.ModuleType | None = ..., /
        ) -> ModuleSpec | None: ...

    class PathEntryFinder(metaclass=ABCMeta):
        """Abstract base class for path entry finders used by PathFinder."""

        if sys.version_info < (3, 12):
            @deprecated("Deprecated since Python 3.4; removed in Python 3.12. Use `PathEntryFinder.find_spec()` instead.")
            def find_module(self, fullname: str) -> Loader | None:
                """Try to find a loader for the specified module by delegating to
                self.find_loader().

                This method is deprecated in favor of finder.find_spec().

                """

            @deprecated("Deprecated since Python 3.4; removed in Python 3.12. Use `find_spec()` instead.")
            def find_loader(self, fullname: str) -> tuple[Loader | None, Sequence[str]]:
                """Return (loader, namespace portion) for the path entry.

                The fullname is a str.  The namespace portion is a sequence of
                path entries contributing to part of a namespace package. The
                sequence may be empty.  If loader is not None, the portion will
                be ignored.

                The portion will be discarded if another path entry finder
                locates the module as a normal module or package.

                This method is deprecated since Python 3.4 in favor of
                finder.find_spec(). If find_spec() is provided than backwards-compatible
                functionality is provided.
                """

        def invalidate_caches(self) -> None:
            """An optional method for clearing the finder's cache, if any.
            This method is used by PathFinder.invalidate_caches().
            """
        # Not defined on the actual class, but expected to exist.
        def find_spec(self, fullname: str, target: types.ModuleType | None = ...) -> ModuleSpec | None: ...

else:
    # Please keep in sync with _typeshed.importlib.MetaPathFinderProtocol
    class MetaPathFinder(Finder):
        """Abstract base class for import finders on sys.meta_path."""

        def find_module(self, fullname: str, path: Sequence[str] | None) -> Loader | None:
            """Return a loader for the module.

            If no module is found, return None.  The fullname is a str and
            the path is a list of strings or None.

            This method is deprecated since Python 3.4 in favor of
            finder.find_spec(). If find_spec() exists then backwards-compatible
            functionality is provided for this method.

            """

        def invalidate_caches(self) -> None:
            """An optional method for clearing the finder's cache, if any.
            This method is used by importlib.invalidate_caches().
            """
        # Not defined on the actual class, but expected to exist.
        def find_spec(
            self, fullname: str, path: Sequence[str] | None, target: types.ModuleType | None = ..., /
        ) -> ModuleSpec | None: ...

    class PathEntryFinder(Finder):
        """Abstract base class for path entry finders used by PathFinder."""

        def find_module(self, fullname: str) -> Loader | None:
            """Try to find a loader for the specified module by delegating to
            self.find_loader().

            This method is deprecated in favor of finder.find_spec().

            """

        def find_loader(self, fullname: str) -> tuple[Loader | None, Sequence[str]]:
            """Return (loader, namespace portion) for the path entry.

            The fullname is a str.  The namespace portion is a sequence of
            path entries contributing to part of a namespace package. The
            sequence may be empty.  If loader is not None, the portion will
            be ignored.

            The portion will be discarded if another path entry finder
            locates the module as a normal module or package.

            This method is deprecated since Python 3.4 in favor of
            finder.find_spec(). If find_spec() is provided than backwards-compatible
            functionality is provided.
            """

        def invalidate_caches(self) -> None:
            """An optional method for clearing the finder's cache, if any.
            This method is used by PathFinder.invalidate_caches().
            """
        # Not defined on the actual class, but expected to exist.
        def find_spec(self, fullname: str, target: types.ModuleType | None = ...) -> ModuleSpec | None: ...

class FileLoader(_bootstrap_external.FileLoader, ResourceLoader, ExecutionLoader, metaclass=ABCMeta):
    """Abstract base class partially implementing the ResourceLoader and
    ExecutionLoader ABCs.
    """

    name: str
    path: str
    def __init__(self, fullname: str, path: str) -> None:
        """Cache the module name and the path to the file found by the
        finder.
        """

    def get_data(self, path: str) -> bytes:
        """Return the data from path as raw bytes."""

    def get_filename(self, fullname: str | None = None) -> str:
        """Return the path to the source file as found by the finder."""

    def load_module(self, fullname: str | None = None) -> types.ModuleType:
        """Load a module from a file.

        This method is deprecated.  Use exec_module() instead.

        """

if sys.version_info < (3, 11):
    class ResourceReader(metaclass=ABCMeta):
        """Abstract base class for loaders to provide resource reading support."""

        @abstractmethod
        def open_resource(self, resource: str) -> IO[bytes]:
            """Return an opened, file-like object for binary reading.

            The 'resource' argument is expected to represent only a file name.
            If the resource cannot be found, FileNotFoundError is raised.
            """

        @abstractmethod
        def resource_path(self, resource: str) -> str:
            """Return the file system path to the specified resource.

            The 'resource' argument is expected to represent only a file name.
            If the resource does not exist on the file system, raise
            FileNotFoundError.
            """
        if sys.version_info >= (3, 10):
            @abstractmethod
            def is_resource(self, path: str) -> bool:
                """Return True if the named 'path' is a resource.

                Files are resources, directories are not.
                """
        else:
            @abstractmethod
            def is_resource(self, name: str) -> bool:
                """Return True if the named 'name' is consider a resource."""

        @abstractmethod
        def contents(self) -> Iterator[str]:
            """Return an iterable of entries in `package`."""

    @runtime_checkable
    class Traversable(Protocol):
        """
        An object with a subset of pathlib.Path methods suitable for
        traversing directories and opening files.
        """

        @abstractmethod
        def is_dir(self) -> bool:
            """
            Return True if self is a dir
            """

        @abstractmethod
        def is_file(self) -> bool:
            """
            Return True if self is a file
            """

        @abstractmethod
        def iterdir(self) -> Iterator[Traversable]:
            """
            Yield Traversable objects in self
            """
        if sys.version_info >= (3, 11):
            @abstractmethod
            def joinpath(self, *descendants: str) -> Traversable: ...
        else:
            @abstractmethod
            def joinpath(self, child: str, /) -> Traversable:
                """
                Return Traversable child in self
                """
        # The documentation and runtime protocol allows *args, **kwargs arguments,
        # but this would mean that all implementers would have to support them,
        # which is not the case.
        @overload
        @abstractmethod
        def open(self, mode: Literal["r"] = "r", *, encoding: str | None = None, errors: str | None = None) -> IO[str]:
            """
            mode may be 'r' or 'rb' to open as text or binary. Return a handle
            suitable for reading (same as pathlib.Path.open).

            When opening as text, accepts encoding parameters such as those
            accepted by io.TextIOWrapper.
            """

        @overload
        @abstractmethod
        def open(self, mode: Literal["rb"]) -> IO[bytes]: ...
        @property
        @abstractmethod
        def name(self) -> str:
            """
            The base name of this object without any parent references.
            """
        if sys.version_info >= (3, 10):
            def __truediv__(self, child: str, /) -> Traversable:
                """
                Return Traversable child in self
                """
        else:
            @abstractmethod
            def __truediv__(self, child: str, /) -> Traversable:
                """
                Return Traversable child in self
                """

        @abstractmethod
        def read_bytes(self) -> bytes:
            """
            Read contents of self as bytes
            """

        @abstractmethod
        def read_text(self, encoding: str | None = None) -> str:
            """
            Read contents of self as text
            """

    class TraversableResources(ResourceReader):
        """
        The required interface for providing traversable
        resources.
        """

        @abstractmethod
        def files(self) -> Traversable:
            """Return a Traversable object for the loaded package."""

        def open_resource(self, resource: str) -> BufferedReader: ...
        def resource_path(self, resource: Any) -> str: ...
        def is_resource(self, path: str) -> bool: ...
        def contents(self) -> Iterator[str]: ...

elif sys.version_info < (3, 14):
    from importlib.resources.abc import (
        ResourceReader as ResourceReader,
        Traversable as Traversable,
        TraversableResources as TraversableResources,
    )
