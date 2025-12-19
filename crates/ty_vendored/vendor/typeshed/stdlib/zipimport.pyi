"""zipimport provides support for importing Python modules from Zip archives.

This module exports two objects:
- zipimporter: a class; its constructor takes a path to a Zip archive.
- ZipImportError: exception raised by zipimporter objects. It's a
  subclass of ImportError, so it can be caught as ImportError, too.

It is usually not needed to use the zipimport module explicitly; it is
used by the builtin import mechanism for sys.path items that are paths
to Zip archives.
"""

import sys
from _typeshed import StrOrBytesPath
from importlib.machinery import ModuleSpec
from types import CodeType, ModuleType
from typing_extensions import deprecated

if sys.version_info >= (3, 10):
    from importlib.readers import ZipReader
else:
    from importlib.abc import ResourceReader

if sys.version_info >= (3, 10):
    from _frozen_importlib_external import _LoaderBasics
else:
    _LoaderBasics = object

__all__ = ["ZipImportError", "zipimporter"]

class ZipImportError(ImportError): ...

class zipimporter(_LoaderBasics):
    """zipimporter(archivepath) -> zipimporter object

    Create a new zipimporter instance. 'archivepath' must be a path to
    a zipfile, or to a specific path inside a zipfile. For example, it can be
    '/tmp/myimport.zip', or '/tmp/myimport.zip/mydirectory', if mydirectory is a
    valid directory inside the archive.

    'ZipImportError is raised if 'archivepath' doesn't point to a valid Zip
    archive.

    The 'archive' attribute of zipimporter objects contains the name of the
    zipfile targeted.
    """

    archive: str
    prefix: str
    if sys.version_info >= (3, 11):
        def __init__(self, path: str) -> None: ...
    else:
        def __init__(self, path: StrOrBytesPath) -> None: ...

    if sys.version_info < (3, 12):
        if sys.version_info >= (3, 10):
            @deprecated("Deprecated since Python 3.10; removed in Python 3.12. Use `find_spec()` instead.")
            def find_loader(self, fullname: str, path: str | None = None) -> tuple[zipimporter | None, list[str]]:
                """find_loader(fullname, path=None) -> self, str or None.

                Search for a module specified by 'fullname'. 'fullname' must be the
                fully qualified (dotted) module name. It returns the zipimporter
                instance itself if the module was found, a string containing the
                full path name if it's possibly a portion of a namespace package,
                or None otherwise. The optional 'path' argument is ignored -- it's
                there for compatibility with the importer protocol.

                Deprecated since Python 3.10. Use find_spec() instead.
                """

            @deprecated("Deprecated since Python 3.10; removed in Python 3.12. Use `find_spec()` instead.")
            def find_module(self, fullname: str, path: str | None = None) -> zipimporter | None:
                """find_module(fullname, path=None) -> self or None.

                Search for a module specified by 'fullname'. 'fullname' must be the
                fully qualified (dotted) module name. It returns the zipimporter
                instance itself if the module was found, or None if it wasn't.
                The optional 'path' argument is ignored -- it's there for compatibility
                with the importer protocol.

                Deprecated since Python 3.10. Use find_spec() instead.
                """
        else:
            def find_loader(self, fullname: str, path: str | None = None) -> tuple[zipimporter | None, list[str]]:
                """find_loader(fullname, path=None) -> self, str or None.

                Search for a module specified by 'fullname'. 'fullname' must be the
                fully qualified (dotted) module name. It returns the zipimporter
                instance itself if the module was found, a string containing the
                full path name if it's possibly a portion of a namespace package,
                or None otherwise. The optional 'path' argument is ignored -- it's
                there for compatibility with the importer protocol.
                """

            def find_module(self, fullname: str, path: str | None = None) -> zipimporter | None:
                """find_module(fullname, path=None) -> self or None.

                Search for a module specified by 'fullname'. 'fullname' must be the
                fully qualified (dotted) module name. It returns the zipimporter
                instance itself if the module was found, or None if it wasn't.
                The optional 'path' argument is ignored -- it's there for compatibility
                with the importer protocol.
                """

    def get_code(self, fullname: str) -> CodeType:
        """get_code(fullname) -> code object.

        Return the code object for the specified module. Raise ZipImportError
        if the module couldn't be imported.
        """

    def get_data(self, pathname: str) -> bytes:
        """get_data(pathname) -> string with file data.

        Return the data associated with 'pathname'. Raise OSError if
        the file wasn't found.
        """

    def get_filename(self, fullname: str) -> str:
        """get_filename(fullname) -> filename string.

        Return the filename for the specified module or raise ZipImportError
        if it couldn't be imported.
        """
    if sys.version_info >= (3, 14):
        def get_resource_reader(self, fullname: str) -> ZipReader:  # undocumented
            """Return the ResourceReader for a module in a zip file."""
    elif sys.version_info >= (3, 10):
        def get_resource_reader(self, fullname: str) -> ZipReader | None:  # undocumented
            """Return the ResourceReader for a module in a zip file."""
    else:
        def get_resource_reader(self, fullname: str) -> ResourceReader | None:  # undocumented
            """Return the ResourceReader for a package in a zip file.

            If 'fullname' is a package within the zip file, return the
            'ResourceReader' object for the package.  Otherwise return None.
            """

    def get_source(self, fullname: str) -> str | None:
        """get_source(fullname) -> source string.

        Return the source code for the specified module. Raise ZipImportError
        if the module couldn't be found, return None if the archive does
        contain the module, but has no source for it.
        """

    def is_package(self, fullname: str) -> bool:
        """is_package(fullname) -> bool.

        Return True if the module specified by fullname is a package.
        Raise ZipImportError if the module couldn't be found.
        """
    if sys.version_info >= (3, 10):
        @deprecated("Deprecated since Python 3.10; removed in Python 3.15. Use `exec_module()` instead.")
        def load_module(self, fullname: str) -> ModuleType:
            """load_module(fullname) -> module.

            Load the module specified by 'fullname'. 'fullname' must be the
            fully qualified (dotted) module name. It returns the imported
            module, or raises ZipImportError if it could not be imported.

            Deprecated since Python 3.10. Use exec_module() instead.
            """

        def exec_module(self, module: ModuleType) -> None:
            """Execute the module."""

        def create_module(self, spec: ModuleSpec) -> None:
            """Use default semantics for module creation."""

        def find_spec(self, fullname: str, target: ModuleType | None = None) -> ModuleSpec | None:
            """Create a ModuleSpec for the specified module.

            Returns None if the module cannot be found.
            """

        def invalidate_caches(self) -> None:
            """Invalidates the cache of file data of the archive path."""
    else:
        def load_module(self, fullname: str) -> ModuleType:
            """load_module(fullname) -> module.

            Load the module specified by 'fullname'. 'fullname' must be the
            fully qualified (dotted) module name. It returns the imported
            module, or raises ZipImportError if it wasn't found.
            """
