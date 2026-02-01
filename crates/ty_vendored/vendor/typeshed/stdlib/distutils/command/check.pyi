"""distutils.command.check

Implements the Distutils 'check' command.
"""

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
    """This command checks the meta-data of the package."""

    description: str
    user_options: ClassVar[list[tuple[str, str, str]]]
    boolean_options: ClassVar[list[str]]
    restructuredtext: int
    metadata: int
    strict: int
    def initialize_options(self) -> None:
        """Sets default values for options."""

    def finalize_options(self) -> None: ...
    def warn(self, msg):
        """Counts the number of warnings that occurs."""

    def run(self) -> None:
        """Runs the command."""

    def check_metadata(self) -> None:
        """Ensures that all required elements of meta-data are supplied.

        Required fields:
            name, version, URL

        Recommended fields:
            (author and author_email) or (maintainer and maintainer_email)

        Warns if any are missing.
        """

    def check_restructuredtext(self) -> None:
        """Checks if the long string fields are reST-compliant."""
