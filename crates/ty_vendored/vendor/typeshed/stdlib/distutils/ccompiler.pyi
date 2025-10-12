"""distutils.ccompiler

Contains CCompiler, an abstract base class that defines the interface
for the Distutils compiler abstraction model.
"""

from _typeshed import BytesPath, StrPath, Unused
from collections.abc import Callable, Iterable, Sequence
from distutils.file_util import _BytesPathT, _StrPathT
from typing import Literal, overload
from typing_extensions import TypeAlias, TypeVarTuple, Unpack

_Macro: TypeAlias = tuple[str] | tuple[str, str | None]
_Ts = TypeVarTuple("_Ts")

def gen_lib_options(
    compiler: CCompiler, library_dirs: list[str], runtime_library_dirs: list[str], libraries: list[str]
) -> list[str]:
    """Generate linker options for searching library directories and
    linking with specific libraries.  'libraries' and 'library_dirs' are,
    respectively, lists of library names (not filenames!) and search
    directories.  Returns a list of command-line options suitable for use
    with some compiler (depending on the two format strings passed in).
    """

def gen_preprocess_options(macros: list[_Macro], include_dirs: list[str]) -> list[str]:
    """Generate C pre-processor options (-D, -U, -I) as used by at least
    two types of compilers: the typical Unix compiler and Visual C++.
    'macros' is the usual thing, a list of 1- or 2-tuples, where (name,)
    means undefine (-U) macro 'name', and (name,value) means define (-D)
    macro 'name' to 'value'.  'include_dirs' is just a list of directory
    names to be added to the header file search path (-I).  Returns a list
    of command-line options suitable for either Unix compilers or Visual
    C++.
    """

def get_default_compiler(osname: str | None = None, platform: str | None = None) -> str:
    """Determine the default compiler to use for the given platform.

    osname should be one of the standard Python OS names (i.e. the
    ones returned by os.name) and platform the common value
    returned by sys.platform for the platform in question.

    The default values are os.name and sys.platform in case the
    parameters are not given.
    """

def new_compiler(
    plat: str | None = None,
    compiler: str | None = None,
    verbose: bool | Literal[0, 1] = 0,
    dry_run: bool | Literal[0, 1] = 0,
    force: bool | Literal[0, 1] = 0,
) -> CCompiler:
    """Generate an instance of some CCompiler subclass for the supplied
    platform/compiler combination.  'plat' defaults to 'os.name'
    (eg. 'posix', 'nt'), and 'compiler' defaults to the default compiler
    for that platform.  Currently only 'posix' and 'nt' are supported, and
    the default compilers are "traditional Unix interface" (UnixCCompiler
    class) and Visual C++ (MSVCCompiler class).  Note that it's perfectly
    possible to ask for a Unix compiler object under Windows, and a
    Microsoft compiler object under Unix -- if you supply a value for
    'compiler', 'plat' is ignored.
    """

def show_compilers() -> None:
    """Print list of available compilers (used by the "--help-compiler"
    options to "build", "build_ext", "build_clib").
    """

class CCompiler:
    """Abstract base class to define the interface that must be implemented
    by real compiler classes.  Also has some utility methods used by
    several compiler classes.

    The basic idea behind a compiler abstraction class is that each
    instance can be used for all the compile/link steps in building a
    single project.  Thus, attributes common to all of those compile and
    link steps -- include directories, macros to define, libraries to link
    against, etc. -- are attributes of the compiler instance.  To allow for
    variability in how individual files are treated, most of those
    attributes may be varied on a per-compilation or per-link basis.
    """

    dry_run: bool
    force: bool
    verbose: bool
    output_dir: str | None
    macros: list[_Macro]
    include_dirs: list[str]
    libraries: list[str]
    library_dirs: list[str]
    runtime_library_dirs: list[str]
    objects: list[str]
    def __init__(
        self, verbose: bool | Literal[0, 1] = 0, dry_run: bool | Literal[0, 1] = 0, force: bool | Literal[0, 1] = 0
    ) -> None: ...
    def add_include_dir(self, dir: str) -> None:
        """Add 'dir' to the list of directories that will be searched for
        header files.  The compiler is instructed to search directories in
        the order in which they are supplied by successive calls to
        'add_include_dir()'.
        """

    def set_include_dirs(self, dirs: list[str]) -> None:
        """Set the list of directories that will be searched to 'dirs' (a
        list of strings).  Overrides any preceding calls to
        'add_include_dir()'; subsequence calls to 'add_include_dir()' add
        to the list passed to 'set_include_dirs()'.  This does not affect
        any list of standard include directories that the compiler may
        search by default.
        """

    def add_library(self, libname: str) -> None:
        """Add 'libname' to the list of libraries that will be included in
        all links driven by this compiler object.  Note that 'libname'
        should *not* be the name of a file containing a library, but the
        name of the library itself: the actual filename will be inferred by
        the linker, the compiler, or the compiler class (depending on the
        platform).

        The linker will be instructed to link against libraries in the
        order they were supplied to 'add_library()' and/or
        'set_libraries()'.  It is perfectly valid to duplicate library
        names; the linker will be instructed to link against libraries as
        many times as they are mentioned.
        """

    def set_libraries(self, libnames: list[str]) -> None:
        """Set the list of libraries to be included in all links driven by
        this compiler object to 'libnames' (a list of strings).  This does
        not affect any standard system libraries that the linker may
        include by default.
        """

    def add_library_dir(self, dir: str) -> None:
        """Add 'dir' to the list of directories that will be searched for
        libraries specified to 'add_library()' and 'set_libraries()'.  The
        linker will be instructed to search for libraries in the order they
        are supplied to 'add_library_dir()' and/or 'set_library_dirs()'.
        """

    def set_library_dirs(self, dirs: list[str]) -> None:
        """Set the list of library search directories to 'dirs' (a list of
        strings).  This does not affect any standard library search path
        that the linker may search by default.
        """

    def add_runtime_library_dir(self, dir: str) -> None:
        """Add 'dir' to the list of directories that will be searched for
        shared libraries at runtime.
        """

    def set_runtime_library_dirs(self, dirs: list[str]) -> None:
        """Set the list of directories to search for shared libraries at
        runtime to 'dirs' (a list of strings).  This does not affect any
        standard search path that the runtime linker may search by
        default.
        """

    def define_macro(self, name: str, value: str | None = None) -> None:
        """Define a preprocessor macro for all compilations driven by this
        compiler object.  The optional parameter 'value' should be a
        string; if it is not supplied, then the macro will be defined
        without an explicit value and the exact outcome depends on the
        compiler used (XXX true? does ANSI say anything about this?)
        """

    def undefine_macro(self, name: str) -> None:
        """Undefine a preprocessor macro for all compilations driven by
        this compiler object.  If the same macro is defined by
        'define_macro()' and undefined by 'undefine_macro()' the last call
        takes precedence (including multiple redefinitions or
        undefinitions).  If the macro is redefined/undefined on a
        per-compilation basis (ie. in the call to 'compile()'), then that
        takes precedence.
        """

    def add_link_object(self, object: str) -> None:
        """Add 'object' to the list of object files (or analogues, such as
        explicitly named library files or the output of "resource
        compilers") to be included in every link driven by this compiler
        object.
        """

    def set_link_objects(self, objects: list[str]) -> None:
        """Set the list of object files (or analogues) to be included in
        every link to 'objects'.  This does not affect any standard object
        files that the linker may include by default (such as system
        libraries).
        """

    def detect_language(self, sources: str | list[str]) -> str | None:
        """Detect the language of a given file, or list of files. Uses
        language_map, and language_order to do the job.
        """

    def find_library_file(self, dirs: list[str], lib: str, debug: bool | Literal[0, 1] = 0) -> str | None:
        """Search the specified list of directories for a static or shared
        library file 'lib' and return the full path to that file.  If
        'debug' true, look for a debugging version (if that makes sense on
        the current platform).  Return None if 'lib' wasn't found in any of
        the specified directories.
        """

    def has_function(
        self,
        funcname: str,
        includes: list[str] | None = None,
        include_dirs: list[str] | None = None,
        libraries: list[str] | None = None,
        library_dirs: list[str] | None = None,
    ) -> bool:
        """Return a boolean indicating whether funcname is supported on
        the current platform.  The optional arguments can be used to
        augment the compilation environment.
        """

    def library_dir_option(self, dir: str) -> str:
        """Return the compiler option to add 'dir' to the list of
        directories searched for libraries.
        """

    def library_option(self, lib: str) -> str:
        """Return the compiler option to add 'lib' to the list of libraries
        linked into the shared library or executable.
        """

    def runtime_library_dir_option(self, dir: str) -> str:
        """Return the compiler option to add 'dir' to the list of
        directories searched for runtime libraries.
        """

    def set_executables(self, **args: str) -> None:
        """Define the executables (and options for them) that will be run
        to perform the various stages of compilation.  The exact set of
        executables that may be specified here depends on the compiler
        class (via the 'executables' class attribute), but most will have:
          compiler      the C/C++ compiler
          linker_so     linker used to create shared objects and libraries
          linker_exe    linker used to create binary executables
          archiver      static library creator

        On platforms with a command-line (Unix, DOS/Windows), each of these
        is a string that will be split into executable name and (optional)
        list of arguments.  (Splitting the string is done similarly to how
        Unix shells operate: words are delimited by spaces, but quotes and
        backslashes can override this.  See
        'distutils.util.split_quoted()'.)
        """

    def compile(
        self,
        sources: Sequence[StrPath],
        output_dir: str | None = None,
        macros: list[_Macro] | None = None,
        include_dirs: list[str] | None = None,
        debug: bool | Literal[0, 1] = 0,
        extra_preargs: list[str] | None = None,
        extra_postargs: list[str] | None = None,
        depends: list[str] | None = None,
    ) -> list[str]:
        """Compile one or more source files.

        'sources' must be a list of filenames, most likely C/C++
        files, but in reality anything that can be handled by a
        particular compiler and compiler class (eg. MSVCCompiler can
        handle resource files in 'sources').  Return a list of object
        filenames, one per source filename in 'sources'.  Depending on
        the implementation, not all source files will necessarily be
        compiled, but all corresponding object filenames will be
        returned.

        If 'output_dir' is given, object files will be put under it, while
        retaining their original path component.  That is, "foo/bar.c"
        normally compiles to "foo/bar.o" (for a Unix implementation); if
        'output_dir' is "build", then it would compile to
        "build/foo/bar.o".

        'macros', if given, must be a list of macro definitions.  A macro
        definition is either a (name, value) 2-tuple or a (name,) 1-tuple.
        The former defines a macro; if the value is None, the macro is
        defined without an explicit value.  The 1-tuple case undefines a
        macro.  Later definitions/redefinitions/ undefinitions take
        precedence.

        'include_dirs', if given, must be a list of strings, the
        directories to add to the default include file search path for this
        compilation only.

        'debug' is a boolean; if true, the compiler will be instructed to
        output debug symbols in (or alongside) the object file(s).

        'extra_preargs' and 'extra_postargs' are implementation- dependent.
        On platforms that have the notion of a command-line (e.g. Unix,
        DOS/Windows), they are most likely lists of strings: extra
        command-line arguments to prepend/append to the compiler command
        line.  On other platforms, consult the implementation class
        documentation.  In any event, they are intended as an escape hatch
        for those occasions when the abstract compiler framework doesn't
        cut the mustard.

        'depends', if given, is a list of filenames that all targets
        depend on.  If a source file is older than any file in
        depends, then the source file will be recompiled.  This
        supports dependency tracking, but only at a coarse
        granularity.

        Raises CompileError on failure.
        """

    def create_static_lib(
        self,
        objects: list[str],
        output_libname: str,
        output_dir: str | None = None,
        debug: bool | Literal[0, 1] = 0,
        target_lang: str | None = None,
    ) -> None:
        """Link a bunch of stuff together to create a static library file.
        The "bunch of stuff" consists of the list of object files supplied
        as 'objects', the extra object files supplied to
        'add_link_object()' and/or 'set_link_objects()', the libraries
        supplied to 'add_library()' and/or 'set_libraries()', and the
        libraries supplied as 'libraries' (if any).

        'output_libname' should be a library name, not a filename; the
        filename will be inferred from the library name.  'output_dir' is
        the directory where the library file will be put.

        'debug' is a boolean; if true, debugging information will be
        included in the library (note that on most platforms, it is the
        compile step where this matters: the 'debug' flag is included here
        just for consistency).

        'target_lang' is the target language for which the given objects
        are being compiled. This allows specific linkage time treatment of
        certain languages.

        Raises LibError on failure.
        """

    def link(
        self,
        target_desc: str,
        objects: list[str],
        output_filename: str,
        output_dir: str | None = None,
        libraries: list[str] | None = None,
        library_dirs: list[str] | None = None,
        runtime_library_dirs: list[str] | None = None,
        export_symbols: list[str] | None = None,
        debug: bool | Literal[0, 1] = 0,
        extra_preargs: list[str] | None = None,
        extra_postargs: list[str] | None = None,
        build_temp: str | None = None,
        target_lang: str | None = None,
    ) -> None:
        """Link a bunch of stuff together to create an executable or
        shared library file.

        The "bunch of stuff" consists of the list of object files supplied
        as 'objects'.  'output_filename' should be a filename.  If
        'output_dir' is supplied, 'output_filename' is relative to it
        (i.e. 'output_filename' can provide directory components if
        needed).

        'libraries' is a list of libraries to link against.  These are
        library names, not filenames, since they're translated into
        filenames in a platform-specific way (eg. "foo" becomes "libfoo.a"
        on Unix and "foo.lib" on DOS/Windows).  However, they can include a
        directory component, which means the linker will look in that
        specific directory rather than searching all the normal locations.

        'library_dirs', if supplied, should be a list of directories to
        search for libraries that were specified as bare library names
        (ie. no directory component).  These are on top of the system
        default and those supplied to 'add_library_dir()' and/or
        'set_library_dirs()'.  'runtime_library_dirs' is a list of
        directories that will be embedded into the shared library and used
        to search for other shared libraries that *it* depends on at
        run-time.  (This may only be relevant on Unix.)

        'export_symbols' is a list of symbols that the shared library will
        export.  (This appears to be relevant only on Windows.)

        'debug' is as for 'compile()' and 'create_static_lib()', with the
        slight distinction that it actually matters on most platforms (as
        opposed to 'create_static_lib()', which includes a 'debug' flag
        mostly for form's sake).

        'extra_preargs' and 'extra_postargs' are as for 'compile()' (except
        of course that they supply command-line arguments for the
        particular linker being used).

        'target_lang' is the target language for which the given objects
        are being compiled. This allows specific linkage time treatment of
        certain languages.

        Raises LinkError on failure.
        """

    def link_executable(
        self,
        objects: list[str],
        output_progname: str,
        output_dir: str | None = None,
        libraries: list[str] | None = None,
        library_dirs: list[str] | None = None,
        runtime_library_dirs: list[str] | None = None,
        debug: bool | Literal[0, 1] = 0,
        extra_preargs: list[str] | None = None,
        extra_postargs: list[str] | None = None,
        target_lang: str | None = None,
    ) -> None: ...
    def link_shared_lib(
        self,
        objects: list[str],
        output_libname: str,
        output_dir: str | None = None,
        libraries: list[str] | None = None,
        library_dirs: list[str] | None = None,
        runtime_library_dirs: list[str] | None = None,
        export_symbols: list[str] | None = None,
        debug: bool | Literal[0, 1] = 0,
        extra_preargs: list[str] | None = None,
        extra_postargs: list[str] | None = None,
        build_temp: str | None = None,
        target_lang: str | None = None,
    ) -> None: ...
    def link_shared_object(
        self,
        objects: list[str],
        output_filename: str,
        output_dir: str | None = None,
        libraries: list[str] | None = None,
        library_dirs: list[str] | None = None,
        runtime_library_dirs: list[str] | None = None,
        export_symbols: list[str] | None = None,
        debug: bool | Literal[0, 1] = 0,
        extra_preargs: list[str] | None = None,
        extra_postargs: list[str] | None = None,
        build_temp: str | None = None,
        target_lang: str | None = None,
    ) -> None: ...
    def preprocess(
        self,
        source: str,
        output_file: str | None = None,
        macros: list[_Macro] | None = None,
        include_dirs: list[str] | None = None,
        extra_preargs: list[str] | None = None,
        extra_postargs: list[str] | None = None,
    ) -> None:
        """Preprocess a single C/C++ source file, named in 'source'.
        Output will be written to file named 'output_file', or stdout if
        'output_file' not supplied.  'macros' is a list of macro
        definitions as for 'compile()', which will augment the macros set
        with 'define_macro()' and 'undefine_macro()'.  'include_dirs' is a
        list of directory names that will be added to the default list.

        Raises PreprocessError on failure.
        """

    @overload
    def executable_filename(self, basename: str, strip_dir: Literal[0, False] = 0, output_dir: StrPath = "") -> str: ...
    @overload
    def executable_filename(self, basename: StrPath, strip_dir: Literal[1, True], output_dir: StrPath = "") -> str: ...
    def library_filename(
        self, libname: str, lib_type: str = "static", strip_dir: bool | Literal[0, 1] = 0, output_dir: StrPath = ""
    ) -> str: ...
    def object_filenames(
        self, source_filenames: Iterable[StrPath], strip_dir: bool | Literal[0, 1] = 0, output_dir: StrPath | None = ""
    ) -> list[str]: ...
    @overload
    def shared_object_filename(self, basename: str, strip_dir: Literal[0, False] = 0, output_dir: StrPath = "") -> str: ...
    @overload
    def shared_object_filename(self, basename: StrPath, strip_dir: Literal[1, True], output_dir: StrPath = "") -> str: ...
    def execute(
        self, func: Callable[[Unpack[_Ts]], Unused], args: tuple[Unpack[_Ts]], msg: str | None = None, level: int = 1
    ) -> None: ...
    def spawn(self, cmd: Iterable[str]) -> None: ...
    def mkpath(self, name: str, mode: int = 0o777) -> None: ...
    @overload
    def move_file(self, src: StrPath, dst: _StrPathT) -> _StrPathT | str: ...
    @overload
    def move_file(self, src: BytesPath, dst: _BytesPathT) -> _BytesPathT | bytes: ...
    def announce(self, msg: str, level: int = 1) -> None: ...
    def warn(self, msg: str) -> None: ...
    def debug_print(self, msg: str) -> None: ...
