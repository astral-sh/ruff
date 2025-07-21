"""distutils.command.build_scripts

Implements the Distutils 'build_scripts' command.
"""

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
    def copy_scripts(self):
        """Copy each script listed in 'self.scripts'; if it's marked as a
        Python script in the Unix way (first line matches 'first_line_re',
        ie. starts with "\\#!" and contains "python"), then adjust the first
        line to refer to the current Python interpreter as we copy.
        """

class build_scripts_2to3(build_scripts, Mixin2to3):
    def copy_scripts(self): ...
