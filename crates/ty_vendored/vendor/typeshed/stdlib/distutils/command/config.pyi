"""distutils.command.config

Implements the Distutils 'config' command, a (mostly) empty command class
that exists mainly to be sub-classed by specific module distributions and
applications.  The idea is that while every "config" command is different,
at least they're all named the same, and users always see "config" in the
list of standard commands.  Also, this is a good place to put common
configure-like tasks: "try to compile this C code", or "figure out where
this header file lives".
"""

from _typeshed import StrOrBytesPath
from collections.abc import Sequence
from re import Pattern
from typing import ClassVar, Final, Literal

from ..ccompiler import CCompiler
from ..cmd import Command

LANG_EXT: Final[dict[str, str]]

class config(Command):
    description: str
    # Tuple is full name, short name, description
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    compiler: str | CCompiler
    cc: str | None
    include_dirs: Sequence[str] | None
    libraries: Sequence[str] | None
    library_dirs: Sequence[str] | None
    noisy: int
    dump_source: int
    temp_files: Sequence[str]
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
    def try_cpp(
        self,
        body: str | None = None,
        headers: Sequence[str] | None = None,
        include_dirs: Sequence[str] | None = None,
        lang: str = "c",
    ) -> bool:
        """Construct a source file from 'body' (a string containing lines
        of C/C++ code) and 'headers' (a list of header files to include)
        and run it through the preprocessor.  Return true if the
        preprocessor succeeded, false if there were any errors.
        ('body' probably isn't of much use, but what the heck.)
        """

    def search_cpp(
        self,
        pattern: Pattern[str] | str,
        body: str | None = None,
        headers: Sequence[str] | None = None,
        include_dirs: Sequence[str] | None = None,
        lang: str = "c",
    ) -> bool:
        """Construct a source file (just like 'try_cpp()'), run it through
        the preprocessor, and return true if any line of the output matches
        'pattern'.  'pattern' should either be a compiled regex object or a
        string containing a regex.  If both 'body' and 'headers' are None,
        preprocesses an empty file -- which can be useful to determine the
        symbols the preprocessor and compiler set by default.
        """

    def try_compile(
        self, body: str, headers: Sequence[str] | None = None, include_dirs: Sequence[str] | None = None, lang: str = "c"
    ) -> bool:
        """Try to compile a source file built from 'body' and 'headers'.
        Return true on success, false otherwise.
        """

    def try_link(
        self,
        body: str,
        headers: Sequence[str] | None = None,
        include_dirs: Sequence[str] | None = None,
        libraries: Sequence[str] | None = None,
        library_dirs: Sequence[str] | None = None,
        lang: str = "c",
    ) -> bool:
        """Try to compile and link a source file, built from 'body' and
        'headers', to executable form.  Return true on success, false
        otherwise.
        """

    def try_run(
        self,
        body: str,
        headers: Sequence[str] | None = None,
        include_dirs: Sequence[str] | None = None,
        libraries: Sequence[str] | None = None,
        library_dirs: Sequence[str] | None = None,
        lang: str = "c",
    ) -> bool:
        """Try to compile, link to an executable, and run a program
        built from 'body' and 'headers'.  Return true on success, false
        otherwise.
        """

    def check_func(
        self,
        func: str,
        headers: Sequence[str] | None = None,
        include_dirs: Sequence[str] | None = None,
        libraries: Sequence[str] | None = None,
        library_dirs: Sequence[str] | None = None,
        decl: bool | Literal[0, 1] = 0,
        call: bool | Literal[0, 1] = 0,
    ) -> bool:
        """Determine if function 'func' is available by constructing a
        source file that refers to 'func', and compiles and links it.
        If everything succeeds, returns true; otherwise returns false.

        The constructed source file starts out by including the header
        files listed in 'headers'.  If 'decl' is true, it then declares
        'func' (as "int func()"); you probably shouldn't supply 'headers'
        and set 'decl' true in the same call, or you might get errors about
        a conflicting declarations for 'func'.  Finally, the constructed
        'main()' function either references 'func' or (if 'call' is true)
        calls it.  'libraries' and 'library_dirs' are used when
        linking.
        """

    def check_lib(
        self,
        library: str,
        library_dirs: Sequence[str] | None = None,
        headers: Sequence[str] | None = None,
        include_dirs: Sequence[str] | None = None,
        other_libraries: list[str] = [],
    ) -> bool:
        """Determine if 'library' is available to be linked against,
        without actually checking that any particular symbols are provided
        by it.  'headers' will be used in constructing the source file to
        be compiled, but the only effect of this is to check if all the
        header files listed are available.  Any libraries listed in
        'other_libraries' will be included in the link, in case 'library'
        has symbols that depend on other libraries.
        """

    def check_header(
        self, header: str, include_dirs: Sequence[str] | None = None, library_dirs: Sequence[str] | None = None, lang: str = "c"
    ) -> bool:
        """Determine if the system header file named by 'header_file'
        exists and can be found by the preprocessor; return true if so,
        false otherwise.
        """

def dump_file(filename: StrOrBytesPath, head=None) -> None:
    """Dumps a file content into log.info.

    If head is not None, will be dumped before the file content.
    """
