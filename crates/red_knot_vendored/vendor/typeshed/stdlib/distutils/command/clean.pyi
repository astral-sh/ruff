from typing import Any, ClassVar

from ..cmd import Command

class clean(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    build_base: Any
    build_lib: Any
    build_temp: Any
    build_scripts: Any
    bdist_base: Any
    all: Any
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
