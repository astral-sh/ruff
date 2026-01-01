"""distutils.pypirc

Provides the PyPIRCCommand class, the base class for the command classes
that uses .pypirc in the distutils.command package.
"""

from abc import abstractmethod
from distutils.cmd import Command
from typing import ClassVar

DEFAULT_PYPIRC: str

class PyPIRCCommand(Command):
    """Base command that knows how to handle the .pypirc file"""

    DEFAULT_REPOSITORY: ClassVar[str]
    DEFAULT_REALM: ClassVar[str]
    repository: None
    realm: None
    user_options: ClassVar[list[tuple[str, str | None, str]]]
    boolean_options: ClassVar[list[str]]
    def initialize_options(self) -> None:
        """Initialize options."""

    def finalize_options(self) -> None:
        """Finalizes options."""

    @abstractmethod
    def run(self) -> None:
        """A command's raison d'etre: carry out the action it exists to
        perform, controlled by the options initialized in
        'initialize_options()', customized by other commands, the setup
        script, the command-line, and config files, and finalized in
        'finalize_options()'.  All terminal output and filesystem
        interaction should be done by 'run()'.

        This method must be implemented by all command classes.
        """
