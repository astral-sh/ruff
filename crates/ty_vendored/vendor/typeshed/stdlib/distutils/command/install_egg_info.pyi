"""distutils.command.install_egg_info

Implements the Distutils 'install_egg_info' command, for installing
a package's PKG-INFO metadata.
"""

from _typeshed import Incomplete
from typing import ClassVar

from ..cmd import Command

class install_egg_info(Command):
    """Install an .egg-info file for the package"""

    description: ClassVar[str]
    user_options: ClassVar[list[tuple[str, str, str]]]
    install_dir: Incomplete
    def initialize_options(self) -> None: ...
    target: Incomplete
    outputs: Incomplete
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
    def get_outputs(self) -> list[str]: ...

def safe_name(name):
    """Convert an arbitrary string to a standard distribution name

    Any runs of non-alphanumeric/. characters are replaced with a single '-'.
    """

def safe_version(version):
    """Convert an arbitrary string to a standard version string

    Spaces become dots, and all other non-alphanumeric characters become
    dashes, with runs of multiple dashes condensed to a single dash.
    """

def to_filename(name):
    """Convert a project or version name to its filename-escaped form

    Any '-' characters are currently replaced with '_'.
    """
