from _typeshed import Incomplete, Unused
from collections.abc import Callable
from typing import Any, ClassVar

from ..cmd import Command

def show_compilers() -> None: ...

class build(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    help_options: ClassVar[list[tuple[str, str | None, str, Callable[[], Unused]]]]
    build_base: str
    build_purelib: Incomplete
    build_platlib: Incomplete
    build_lib: Incomplete
    build_temp: Incomplete
    build_scripts: Incomplete
    compiler: Incomplete
    plat_name: Incomplete
    debug: Incomplete
    force: int
    executable: Incomplete
    parallel: Incomplete
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
    def has_pure_modules(self): ...
    def has_c_libraries(self): ...
    def has_ext_modules(self): ...
    def has_scripts(self): ...
    # Any to work around variance issues
    sub_commands: ClassVar[list[tuple[str, Callable[[Any], bool] | None]]]
