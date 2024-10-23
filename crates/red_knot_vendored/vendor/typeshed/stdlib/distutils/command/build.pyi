from _typeshed import Unused
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
    build_purelib: Any
    build_platlib: Any
    build_lib: Any
    build_temp: Any
    build_scripts: Any
    compiler: Any
    plat_name: Any
    debug: Any
    force: int
    executable: Any
    parallel: Any
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
    def has_pure_modules(self): ...
    def has_c_libraries(self): ...
    def has_ext_modules(self): ...
    def has_scripts(self): ...
    # Any to work around variance issues
    sub_commands: ClassVar[list[tuple[str, Callable[[Any], bool] | None]]]
