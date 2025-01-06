from _typeshed import Incomplete
from typing import ClassVar

from ..cmd import Command

class bdist_dumb(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    default_format: ClassVar[dict[str, str]]
    bdist_dir: Incomplete
    plat_name: Incomplete
    format: Incomplete
    keep_temp: int
    dist_dir: Incomplete
    skip_build: Incomplete
    relative: int
    owner: Incomplete
    group: Incomplete
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
