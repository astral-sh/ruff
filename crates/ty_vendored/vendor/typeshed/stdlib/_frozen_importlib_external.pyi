"""Core implementation of path-based import.

This module is NOT meant to be directly imported! It has been designed such
that it can be bootstrapped into Python as the implementation of import. As
such it requires the injection of specific modules and attributes in order to
work. One should use importlib as the public-facing version of this module.

"""

import _ast
import _io
import importlib.abc
import importlib.machinery
import sys
import types
from _typeshed import ReadableBuffer, StrOrBytesPath, StrPath
from _typeshed.importlib import LoaderProtocol
from collections.abc import Callable, Iterable, Iterator, Mapping, MutableSequence, Sequence
from importlib.machinery import ModuleSpec
from importlib.metadata import DistributionFinder, PathDistribution
from typing import Any, Final, Literal
from typing_extensions import Self, deprecated

if sys.version_info >= (3, 10):
    import importlib.readers

if sys.platform == "win32":
    path_separators: Literal["\\/"]
    path_sep: Literal["\\"]
    path_sep_tuple: tuple[Literal["\\"], Literal["/"]]
else:
    path_separators: Literal["/"]
    path_sep: Literal["/"]
    path_sep_tuple: tuple[Literal["/"]]

MAGIC_NUMBER: Final[bytes]

def cache_from_source(path: StrPath, debug_override: bool | None = None, *, optimization: Any | None = None) -> str:
    """Given the path to a .py file, return the path to its .pyc file.

    The .py file does not need to exist; this simply returns the path to the
    .pyc file calculated as if the .py file were imported.

    The 'optimization' parameter controls the presumed optimization level of
    the bytecode file. If 'optimization' is not None, the string representation
    of the argument is taken and verified to be alphanumeric (else ValueError
    is raised).

    The debug_override parameter is deprecated. If debug_override is not None,
    a True value is the same as setting 'optimization' to the empty string
    while a False value is equivalent to setting 'optimization' to '1'.

    If sys.implementation.cache_tag is None then NotImplementedError is raised.

    """

def source_from_cache(path: StrPath) -> str:
    """Given the path to a .pyc. file, return the path to its .py file.

    The .pyc file does not need to exist; this simply returns the path to
    the .py file calculated to correspond to the .pyc file.  If path does
    not conform to PEP 3147/488 format, ValueError will be raised. If
    sys.implementation.cache_tag is None then NotImplementedError is raised.

    """

def decode_source(source_bytes: ReadableBuffer) -> str:
    """Decode bytes representing source code and return the string.

    Universal newline support is used in the decoding.
    """

def spec_from_file_location(
    name: str,
    location: StrOrBytesPath | None = None,
    *,
    loader: LoaderProtocol | None = None,
    submodule_search_locations: list[str] | None = ...,
) -> importlib.machinery.ModuleSpec | None:
    """Return a module spec based on a file location.

    To indicate that the module is a package, set
    submodule_search_locations to a list of directory paths.  An
    empty list is sufficient, though its not otherwise useful to the
    import system.

    The loader must take a spec as its only __init__() arg.

    """

@deprecated(
    "Deprecated since Python 3.6. Use site configuration instead. "
    "Future versions of Python may not enable this finder by default."
)
class WindowsRegistryFinder(importlib.abc.MetaPathFinder):
    """Meta path finder for modules declared in the Windows registry."""

    if sys.version_info < (3, 12):
        @classmethod
        @deprecated("Deprecated since Python 3.4; removed in Python 3.12. Use `find_spec()` instead.")
        def find_module(cls, fullname: str, path: Sequence[str] | None = None) -> importlib.abc.Loader | None:
            """Find module named in the registry.

            This method is deprecated.  Use find_spec() instead.

            """

    @classmethod
    def find_spec(
        cls, fullname: str, path: Sequence[str] | None = None, target: types.ModuleType | None = None
    ) -> ModuleSpec | None: ...

class PathFinder(importlib.abc.MetaPathFinder):
    """Meta path finder for sys.path and package __path__ attributes."""

    if sys.version_info >= (3, 10):
        @staticmethod
        def invalidate_caches() -> None:
            """Call the invalidate_caches() method on all path entry finders
            stored in sys.path_importer_cache (where implemented).
            """
    else:
        @classmethod
        def invalidate_caches(cls) -> None:
            """Call the invalidate_caches() method on all path entry finders
            stored in sys.path_importer_caches (where implemented).
            """
    if sys.version_info >= (3, 10):
        @staticmethod
        def find_distributions(context: DistributionFinder.Context = ...) -> Iterable[PathDistribution]:
            """
            Find distributions.

            Return an iterable of all Distribution instances capable of
            loading the metadata for packages matching ``context.name``
            (or all names if ``None`` indicated) along the paths in the list
            of directories ``context.path``.
            """
    else:
        @classmethod
        def find_distributions(cls, context: DistributionFinder.Context = ...) -> Iterable[PathDistribution]:
            """
            Find distributions.

            Return an iterable of all Distribution instances capable of
            loading the metadata for packages matching ``context.name``
            (or all names if ``None`` indicated) along the paths in the list
            of directories ``context.path``.
            """

    @classmethod
    def find_spec(
        cls, fullname: str, path: Sequence[str] | None = None, target: types.ModuleType | None = None
    ) -> ModuleSpec | None:
        """Try to find a spec for 'fullname' on sys.path or 'path'.

        The search is based on sys.path_hooks and sys.path_importer_cache.
        """
    if sys.version_info < (3, 12):
        @classmethod
        @deprecated("Deprecated since Python 3.4; removed in Python 3.12. Use `find_spec()` instead.")
        def find_module(cls, fullname: str, path: Sequence[str] | None = None) -> importlib.abc.Loader | None:
            """find the module on sys.path or 'path' based on sys.path_hooks and
            sys.path_importer_cache.

            This method is deprecated.  Use find_spec() instead.

            """

SOURCE_SUFFIXES: Final[list[str]]
DEBUG_BYTECODE_SUFFIXES: Final = [".pyc"]
OPTIMIZED_BYTECODE_SUFFIXES: Final = [".pyc"]
BYTECODE_SUFFIXES: Final = [".pyc"]
EXTENSION_SUFFIXES: Final[list[str]]

class FileFinder(importlib.abc.PathEntryFinder):
    """File-based finder.

    Interactions with the file system are cached for performance, being
    refreshed when the directory the finder is handling has been modified.

    """

    path: str
    def __init__(self, path: str, *loader_details: tuple[type[importlib.abc.Loader], list[str]]) -> None:
        """Initialize with the path to search on and a variable number of
        2-tuples containing the loader and the file suffixes the loader
        recognizes.
        """

    @classmethod
    def path_hook(
        cls, *loader_details: tuple[type[importlib.abc.Loader], list[str]]
    ) -> Callable[[str], importlib.abc.PathEntryFinder]:
        """A class method which returns a closure to use on sys.path_hook
        which will return an instance using the specified loaders and the path
        called on the closure.

        If the path called on the closure is not a directory, ImportError is
        raised.

        """

class _LoaderBasics:
    """Base class of common code needed by both SourceLoader and
    SourcelessFileLoader.
    """

    def is_package(self, fullname: str) -> bool:
        """Concrete implementation of InspectLoader.is_package by checking if
        the path returned by get_filename has a filename of '__init__.py'.
        """

    def create_module(self, spec: ModuleSpec) -> types.ModuleType | None:
        """Use default semantics for module creation."""

    def exec_module(self, module: types.ModuleType) -> None:
        """Execute the module."""

    def load_module(self, fullname: str) -> types.ModuleType:
        """This method is deprecated."""

class SourceLoader(_LoaderBasics):
    def path_mtime(self, path: str) -> float:
        """Optional method that returns the modification time (an int) for the
        specified path (a str).

        Raises OSError when the path cannot be handled.
        """

    def set_data(self, path: str, data: bytes) -> None:
        """Optional method which writes data (bytes) to a file path (a str).

        Implementing this method allows for the writing of bytecode files.
        """

    def get_source(self, fullname: str) -> str | None:
        """Concrete implementation of InspectLoader.get_source."""

    def path_stats(self, path: str) -> Mapping[str, Any]:
        """Optional method returning a metadata dict for the specified
        path (a str).

        Possible keys:
        - 'mtime' (mandatory) is the numeric timestamp of last source
          code modification;
        - 'size' (optional) is the size in bytes of the source code.

        Implementing this method allows the loader to read bytecode files.
        Raises OSError when the path cannot be handled.
        """

    def source_to_code(
        self, data: ReadableBuffer | str | _ast.Module | _ast.Expression | _ast.Interactive, path: bytes | StrPath
    ) -> types.CodeType:
        """Return the code object compiled from source.

        The 'data' argument can be any object type that compile() supports.
        """

    def get_code(self, fullname: str) -> types.CodeType | None:
        """Concrete implementation of InspectLoader.get_code.

        Reading of bytecode requires path_stats to be implemented. To write
        bytecode, set_data must also be implemented.

        """

class FileLoader:
    """Base file loader class which implements the loader protocol methods that
    require file system usage.
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
    if sys.version_info >= (3, 10):
        def get_resource_reader(self, name: str | None = None) -> importlib.readers.FileReader: ...
    else:
        def get_resource_reader(self, name: str | None = None) -> Self | None: ...
        def open_resource(self, resource: str) -> _io.FileIO: ...
        def resource_path(self, resource: str) -> str: ...
        def is_resource(self, name: str) -> bool: ...
        def contents(self) -> Iterator[str]: ...

class SourceFileLoader(importlib.abc.FileLoader, FileLoader, importlib.abc.SourceLoader, SourceLoader):  # type: ignore[misc]  # incompatible method arguments in base classes
    """Concrete implementation of SourceLoader using the file system."""

    def set_data(self, path: str, data: ReadableBuffer, *, _mode: int = 0o666) -> None:
        """Write bytes data to a file."""

    def path_stats(self, path: str) -> Mapping[str, Any]:
        """Return the metadata for the path."""

    def source_to_code(  # type: ignore[override]  # incompatible with InspectLoader.source_to_code
        self,
        data: ReadableBuffer | str | _ast.Module | _ast.Expression | _ast.Interactive,
        path: bytes | StrPath,
        *,
        _optimize: int = -1,
    ) -> types.CodeType:
        """Return the code object compiled from source.

        The 'data' argument can be any object type that compile() supports.
        """

class SourcelessFileLoader(importlib.abc.FileLoader, FileLoader, _LoaderBasics):
    """Loader which handles sourceless file imports."""

    def get_code(self, fullname: str) -> types.CodeType | None: ...
    def get_source(self, fullname: str) -> None:
        """Return None as there is no source code."""

class ExtensionFileLoader(FileLoader, _LoaderBasics, importlib.abc.ExecutionLoader):
    """Loader for extension modules.

    The constructor is designed to work with FileFinder.

    """

    def __init__(self, name: str, path: str) -> None: ...
    def get_filename(self, fullname: str | None = None) -> str:
        """Return the path to the source file as found by the finder."""

    def get_source(self, fullname: str) -> None:
        """Return None as extension modules have no source code."""

    def create_module(self, spec: ModuleSpec) -> types.ModuleType:
        """Create an uninitialized extension module"""

    def exec_module(self, module: types.ModuleType) -> None:
        """Initialize an extension module"""

    def get_code(self, fullname: str) -> None:
        """Return None as an extension module cannot create a code object."""

    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

if sys.version_info >= (3, 11):
    class NamespaceLoader(importlib.abc.InspectLoader):
        def __init__(
            self, name: str, path: MutableSequence[str], path_finder: Callable[[str, tuple[str, ...]], ModuleSpec]
        ) -> None: ...
        def is_package(self, fullname: str) -> Literal[True]: ...
        def get_source(self, fullname: str) -> Literal[""]: ...
        def get_code(self, fullname: str) -> types.CodeType: ...
        def create_module(self, spec: ModuleSpec) -> None:
            """Use default semantics for module creation."""

        def exec_module(self, module: types.ModuleType) -> None: ...
        @deprecated("Deprecated since Python 3.10; will be removed in Python 3.15. Use `exec_module()` instead.")
        def load_module(self, fullname: str) -> types.ModuleType:
            """Load a namespace module.

            This method is deprecated.  Use exec_module() instead.

            """

        def get_resource_reader(self, module: types.ModuleType) -> importlib.readers.NamespaceReader: ...
        if sys.version_info < (3, 12):
            @staticmethod
            @deprecated(
                "Deprecated since Python 3.4; removed in Python 3.12. "
                "The module spec is now used by the import machinery to generate a module repr."
            )
            def module_repr(module: types.ModuleType) -> str:
                """Return repr for the module.

                The method is deprecated.  The import machinery does the job itself.

                """

    _NamespaceLoader = NamespaceLoader
else:
    class _NamespaceLoader:
        def __init__(
            self, name: str, path: MutableSequence[str], path_finder: Callable[[str, tuple[str, ...]], ModuleSpec]
        ) -> None: ...
        def is_package(self, fullname: str) -> Literal[True]: ...
        def get_source(self, fullname: str) -> Literal[""]: ...
        def get_code(self, fullname: str) -> types.CodeType: ...
        def create_module(self, spec: ModuleSpec) -> None:
            """Use default semantics for module creation."""

        def exec_module(self, module: types.ModuleType) -> None: ...
        if sys.version_info >= (3, 10):
            @deprecated("Deprecated since Python 3.10; will be removed in Python 3.15. Use `exec_module()` instead.")
            def load_module(self, fullname: str) -> types.ModuleType:
                """Load a namespace module.

                This method is deprecated.  Use exec_module() instead.

                """

            @staticmethod
            @deprecated(
                "Deprecated since Python 3.4; removed in Python 3.12. "
                "The module spec is now used by the import machinery to generate a module repr."
            )
            def module_repr(module: types.ModuleType) -> str:
                """Return repr for the module.

                The method is deprecated.  The import machinery does the job itself.

                """

            def get_resource_reader(self, module: types.ModuleType) -> importlib.readers.NamespaceReader: ...
        else:
            def load_module(self, fullname: str) -> types.ModuleType:
                """Load a namespace module.

                This method is deprecated.  Use exec_module() instead.

                """

            @classmethod
            @deprecated(
                "Deprecated since Python 3.4; removed in Python 3.12. "
                "The module spec is now used by the import machinery to generate a module repr."
            )
            def module_repr(cls, module: types.ModuleType) -> str:
                """Return repr for the module.

                The method is deprecated.  The import machinery does the job itself.

                """

if sys.version_info >= (3, 13):
    class AppleFrameworkLoader(ExtensionFileLoader, importlib.abc.ExecutionLoader):
        """A loader for modules that have been packaged as frameworks for
        compatibility with Apple's iOS App Store policies.
        """
