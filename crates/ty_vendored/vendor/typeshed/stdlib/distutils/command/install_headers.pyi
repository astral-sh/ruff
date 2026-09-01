from _typeshed import Incomplete
from typing import ClassVar

from ..cmd import Command

class install_headers(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str, str]]]
    boolean_options: ClassVar[list[str]]
    install_dir: Incomplete
    force: int
    outfiles: Incomplete
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
    def get_inputs(self): ...
    def get_outputs(self): ...
