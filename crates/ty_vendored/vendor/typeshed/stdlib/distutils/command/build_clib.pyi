"""distutils.command.build_clib

Implements the Distutils 'build_clib' command, to build a C/C++ library
that is included in the module distribution and needed by an extension
module.
"""

from _typeshed import Incomplete, Unused
from collections.abc import Callable
from typing import ClassVar

from ..cmd import Command

def show_compilers() -> None: ...

class build_clib(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str, str]]]
    boolean_options: ClassVar[list[str]]
    help_options: ClassVar[list[tuple[str, str | None, str, Callable[[], Unused]]]]
    build_clib: Incomplete
    build_temp: Incomplete
    libraries: Incomplete
    include_dirs: Incomplete
    define: Incomplete
    undef: Incomplete
    debug: Incomplete
    force: int
    compiler: Incomplete
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
    def check_library_list(self, libraries) -> None:
        """Ensure that the list of libraries is valid.

        `library` is presumably provided as a command option 'libraries'.
        This method checks that it is a list of 2-tuples, where the tuples
        are (library_name, build_info_dict).

        Raise DistutilsSetupError if the structure is invalid anywhere;
        just returns otherwise.
        """

    def get_library_names(self): ...
    def get_source_files(self): ...
    def build_libraries(self, libraries) -> None: ...
