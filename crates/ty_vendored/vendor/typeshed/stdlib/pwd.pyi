"""This module provides access to the Unix password database.
It is available on all Unix versions.

Password database entries are reported as 7-tuples containing the following
items from the password database (see `<pwd.h>'), in order:
pw_name, pw_passwd, pw_uid, pw_gid, pw_gecos, pw_dir, pw_shell.
The uid and gid items are integers, all others are strings. An
exception is raised if the entry asked for cannot be found.
"""

import sys
from _typeshed import structseq
from typing import Any, Final, final

if sys.platform != "win32":
    @final
    class struct_passwd(structseq[Any], tuple[str, str, int, int, str, str, str]):
        """pwd.struct_passwd: Results from getpw*() routines.

        This object may be accessed either as a tuple of
          (pw_name,pw_passwd,pw_uid,pw_gid,pw_gecos,pw_dir,pw_shell)
        or via the object attributes as named in the above tuple.
        """

        if sys.version_info >= (3, 10):
            __match_args__: Final = ("pw_name", "pw_passwd", "pw_uid", "pw_gid", "pw_gecos", "pw_dir", "pw_shell")

        @property
        def pw_name(self) -> str:
            """user name"""

        @property
        def pw_passwd(self) -> str:
            """password"""

        @property
        def pw_uid(self) -> int:
            """user id"""

        @property
        def pw_gid(self) -> int:
            """group id"""

        @property
        def pw_gecos(self) -> str:
            """real name"""

        @property
        def pw_dir(self) -> str:
            """home directory"""

        @property
        def pw_shell(self) -> str:
            """shell program"""

    def getpwall() -> list[struct_passwd]:
        """Return a list of all available password database entries, in arbitrary order.

        See help(pwd) for more on password database entries.
        """

    def getpwuid(uid: int, /) -> struct_passwd:
        """Return the password database entry for the given numeric user ID.

        See `help(pwd)` for more on password database entries.
        """

    def getpwnam(name: str, /) -> struct_passwd:
        """Return the password database entry for the given user name.

        See `help(pwd)` for more on password database entries.
        """
