"""distutils.command.register

Implements the Distutils 'register' command (register with the repository).
"""

from collections.abc import Callable
from typing import Any, ClassVar

from ..config import PyPIRCCommand

class register(PyPIRCCommand):
    description: str
    # Any to work around variance issues
    sub_commands: ClassVar[list[tuple[str, Callable[[Any], bool] | None]]]
    list_classifiers: int
    strict: int
    def initialize_options(self) -> None: ...
    def finalize_options(self) -> None: ...
    def run(self) -> None: ...
    def check_metadata(self) -> None:
        """Deprecated API."""

    def classifiers(self) -> None:
        """Fetch the list of classifiers from the server."""

    def verify_metadata(self) -> None:
        """Send the metadata to the package index server to be checked."""

    def send_metadata(self) -> None:
        """Send the metadata to the package index server.

        Well, do the following:
        1. figure who the user is, and then
        2. send the data as a Basic auth'ed POST.

        First we try to read the username/password from $HOME/.pypirc,
        which is a ConfigParser-formatted file with a section
        [distutils] containing username and password entries (both
        in clear text). Eg:

            [distutils]
            index-servers =
                pypi

            [pypi]
            username: fred
            password: sekrit

        Otherwise, to figure who the user is, we offer the user three
        choices:

         1. use existing login,
         2. register as a new user, or
         3. set the password to a random string and email the user.

        """

    def build_post_data(self, action): ...
    def post_to_server(self, data, auth=None):
        """Post a query to the server, and return a string response."""
