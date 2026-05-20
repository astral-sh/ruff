"""distutils.command.build_ext

Implements the Distutils 'build_ext' command, for building extension
modules (currently limited to C extensions, should accommodate C++
extensions ASAP).
"""

from _typeshed import Incomplete, Unused
from collections.abc import Callable
from typing import ClassVar

from ..cmd import Command

extension_name_re: Incomplete

def show_compilers() -> None: ...

class build_ext(Command):
    description: str
    sep_by: Incomplete
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    help_options: ClassVar[list[tuple[str, str | None, str, Callable[[], Unused]]]]
    extensions: Incomplete
    build_lib: Incomplete
    plat_name: Incomplete
    build_temp: Incomplete
    inplace: int
    package: Incomplete
    include_dirs: Incomplete
    define: Incomplete
    undef: Incomplete
    libraries: Incomplete
    library_dirs: Incomplete
    rpath: Incomplete
    link_objects: Incomplete
    debug: Incomplete
    force: Incomplete
    compiler: Incomplete
    swig: Incomplete
    swig_cpp: Incomplete
    swig_opts: Incomplete
    user: Incomplete
    parallel: Incomplete
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
    def check_extensions_list(self, extensions) -> None:
        """Ensure that the list of extensions (presumably provided as a
        command option 'extensions') is valid, i.e. it is a list of
        Extension objects.  We also support the old-style list of 2-tuples,
        where the tuples are (ext_name, build_info), which are converted to
        Extension instances here.

        Raise DistutilsSetupError if the structure is invalid anywhere;
        just returns otherwise.
        """

    def get_source_files(self): ...
    def get_outputs(self): ...
    def build_extensions(self) -> None: ...
    def build_extension(self, ext) -> None: ...
    def swig_sources(self, sources, extension):
        """Walk the list of source files in 'sources', looking for SWIG
        interface (.i) files.  Run SWIG on all that are found, and
        return a modified 'sources' list with SWIG source files replaced
        by the generated C (or C++) files.
        """

    def find_swig(self):
        """Return the name of the SWIG executable.  On Unix, this is
        just "swig" -- it should be in the PATH.  Tries a bit harder on
        Windows.
        """

    def get_ext_fullpath(self, ext_name: str) -> str:
        """Returns the path of the filename for a given extension.

        The file is located in `build_lib` or directly in the package
        (inplace option).
        """

    def get_ext_fullname(self, ext_name: str) -> str:
        """Returns the fullname of a given extension name.

        Adds the `package.` prefix
        """

    def get_ext_filename(self, ext_name: str) -> str:
        """Convert the name of an extension (eg. "foo.bar") into the name
        of the file from which it will be loaded (eg. "foo/bar.so", or
        "foo\\bar.pyd").
        """

    def get_export_symbols(self, ext):
        """Return the list of symbols that a shared extension has to
        export.  This either uses 'ext.export_symbols' or, if it's not
        provided, "PyInit_" + module_name.  Only relevant on Windows, where
        the .pyd file (DLL) must export the module "PyInit_" function.
        """

    def get_libraries(self, ext):
        """Return the list of libraries to link against when building a
        shared extension.  On most platforms, this is just 'ext.libraries';
        on Windows, we add the Python library (eg. python20.dll).
        """
