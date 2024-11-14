from _typeshed import Incomplete
from typing import Any, ClassVar, Final, Literal
from typing_extensions import TypeAlias

from ..cmd import Command

_Reporter: TypeAlias = Any  # really docutils.utils.Reporter

# Only defined if docutils is installed.
# Depends on a third-party stub. Since distutils is deprecated anyway,
# it's easier to just suppress the "any subclassing" error.
class SilentReporter(_Reporter):
    messages: Incomplete
    def __init__(
        self,
        source,
        report_level,
        halt_level,
        stream: Incomplete | None = ...,
        debug: bool | Literal[0, 1] = 0,
        encoding: str = ...,
        error_handler: str = ...,
    ) -> None: ...
    def system_message(self, level, message, *children, **kwargs): ...

HAS_DOCUTILS: Final[bool]

class check(Command):
    description: str
    user_options: ClassVar[list[tuple[str, str, str]]]
    boolean_options: ClassVar[list[str]]
    restructuredtext: int
    metadata: int
    strict: int
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def warn(self, msg): ...
    def run(self) -> None: ...
    def check_metadata(self) -> None: ...
    def check_restructuredtext(self) -> None: ...
