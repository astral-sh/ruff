from typing import Any, ClassVar

from ..cmd import Command

class bdist_dumb(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    default_format: ClassVar[dict[str, str]]
    bdist_dir: Any
    plat_name: Any
    format: Any
    keep_temp: int
    dist_dir: Any
    skip_build: Any
    relative: int
    owner: Any
    group: Any
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
