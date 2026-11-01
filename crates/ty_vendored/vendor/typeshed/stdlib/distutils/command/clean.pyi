from _typeshed import Incomplete
from typing import ClassVar

from ..cmd import Command

class clean(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    build_base: Incomplete
    build_lib: Incomplete
    build_temp: Incomplete
    build_scripts: Incomplete
    bdist_base: Incomplete
    all: Incomplete
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
