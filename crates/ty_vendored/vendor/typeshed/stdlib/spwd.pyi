"""This module provides access to the Unix shadow password database.
It is available on various Unix versions.

Shadow password database entries are reported as 9-tuples of type struct_spwd,
containing the following items from the password database (see `<shadow.h>'):
sp_namp, sp_pwdp, sp_lstchg, sp_min, sp_max, sp_warn, sp_inact, sp_expire, sp_flag.
The sp_namp and sp_pwdp are strings, the rest are integers.
An exception is raised if the entry asked for cannot be found.
You have to be root to be able to use this module.
"""

import sys
from _typeshed import structseq
from typing import Any, Final, final

if sys.platform != "win32":
    @final
    class struct_spwd(structseq[Any], tuple[str, str, int, int, int, int, int, int, int]):
        """spwd.struct_spwd: Results from getsp*() routines.

        This object may be accessed either as a 9-tuple of
          (sp_namp,sp_pwdp,sp_lstchg,sp_min,sp_max,sp_warn,sp_inact,sp_expire,sp_flag)
        or via the object attributes as named in the above tuple.
        """

        if sys.version_info >= (3, 10):
            __match_args__: Final = (
                "sp_namp",
                "sp_pwdp",
                "sp_lstchg",
                "sp_min",
                "sp_max",
                "sp_warn",
                "sp_inact",
                "sp_expire",
                "sp_flag",
            )

        @property
        def sp_namp(self) -> str:
            """login name"""

        @property
        def sp_pwdp(self) -> str:
            """encrypted password"""

        @property
        def sp_lstchg(self) -> int:
            """date of last change"""

        @property
        def sp_min(self) -> int:
            """min #days between changes"""

        @property
        def sp_max(self) -> int:
            """max #days between changes"""

        @property
        def sp_warn(self) -> int:
            """#days before pw expires to warn user about it"""

        @property
        def sp_inact(self) -> int:
            """#days after pw expires until account is disabled"""

        @property
        def sp_expire(self) -> int:
            """#days since 1970-01-01 when account expires"""

        @property
        def sp_flag(self) -> int:
            """reserved"""
        # Deprecated aliases below.
        @property
        def sp_nam(self) -> str:
            """login name; deprecated"""

        @property
        def sp_pwd(self) -> str:
            """encrypted password; deprecated"""

    def getspall() -> list[struct_spwd]:
        """Return a list of all available shadow password database entries, in arbitrary order.

        See `help(spwd)` for more on shadow password database entries.
        """

    def getspnam(arg: str, /) -> struct_spwd:
        """Return the shadow password database entry for the given user name.

        See `help(spwd)` for more on shadow password database entries.
        """
