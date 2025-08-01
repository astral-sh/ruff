from _typeshed import Incomplete
from typing import ClassVar

from ..cmd import Command
from ..util import Mixin2to3 as Mixin2to3

first_line_re: Incomplete

class build_scripts(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str, str]]]
    boolean_options: ClassVar[list[str]]
    build_dir: Incomplete
    scripts: Incomplete
    force: Incomplete
    executable: Incomplete
    outfiles: Incomplete
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def get_source_files(self): ...
    def run(self) -> None: ...
    def copy_scripts(self): ...

class build_scripts_2to3(build_scripts, Mixin2to3):
    def copy_scripts(self): ...
