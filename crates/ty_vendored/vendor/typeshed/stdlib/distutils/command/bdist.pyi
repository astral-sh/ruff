"""distutils.command.bdist

Implements the Distutils 'bdist' command (create a built [binary]
distribution).
"""

from _typeshed import Incomplete, Unused
from collections.abc import Callable
from typing import ClassVar

from ..cmd import Command

def show_formats() -> None:
    """Print list of available formats (arguments to "--format" option)."""

class bdist(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    help_options: ClassVar[list[tuple[str, str | None, str, Callable[[], Unused]]]]
    no_format_option: ClassVar[tuple[str, ...]]
    default_format: ClassVar[dict[str, str]]
    format_commands: ClassVar[list[str]]
    format_command: ClassVar[dict[str, tuple[str, str]]]
    bdist_base: Incomplete
    plat_name: Incomplete
    formats: Incomplete
    dist_dir: Incomplete
    skip_build: int
    group: Incomplete
    owner: Incomplete
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
