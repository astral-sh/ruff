from _typeshed import Unused
from collections.abc import Callable
from typing import Any, ClassVar

from ..cmd import Command

def show_formats() -> None: ...

class bdist(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    help_options: ClassVar[list[tuple[str, str | None, str, Callable[[], Unused]]]]
    no_format_option: ClassVar[tuple[str, ...]]
    default_format: ClassVar[dict[str, str]]
    format_commands: ClassVar[list[str]]
    format_command: ClassVar[dict[str, tuple[str, str]]]
    bdist_base: Any
    plat_name: Any
    formats: Any
    dist_dir: Any
    skip_build: int
    group: Any
    owner: Any
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
