"""Access to the Unix group database.

Group entries are reported as 4-tuples containing the following fields
from the group database, in order:

  gr_name   - name of the group
  gr_passwd - group password (encrypted); often empty
  gr_gid    - numeric ID of the group
  gr_mem    - list of members

The gid is an integer, name and password are strings.  (Note that most
users are not explicitly listed as members of the groups they are in
according to the password database.  Check both databases to get
complete membership information.)
"""

import sys
from _typeshed import structseq
from typing import Any, Final, final

if sys.platform != "win32":
    @final
    class struct_group(structseq[Any], tuple[str, str | None, int, list[str]]):
        """grp.struct_group: Results from getgr*() routines.

        This object may be accessed either as a tuple of
          (gr_name,gr_passwd,gr_gid,gr_mem)
        or via the object attributes as named in the above tuple.
        """

        if sys.version_info >= (3, 10):
            __match_args__: Final = ("gr_name", "gr_passwd", "gr_gid", "gr_mem")

        @property
        def gr_name(self) -> str:
            """group name"""

        @property
        def gr_passwd(self) -> str | None:
            """password"""

        @property
        def gr_gid(self) -> int:
            """group id"""

        @property
        def gr_mem(self) -> list[str]:
            """group members"""

    def getgrall() -> list[struct_group]:
        """Return a list of all available group entries, in arbitrary order.

        An entry whose name starts with '+' or '-' represents an instruction
        to use YP/NIS and may not be accessible via getgrnam or getgrgid.
        """

    def getgrgid(id: int) -> struct_group:
        """Return the group database entry for the given numeric group ID.

        If id is not valid, raise KeyError.
        """

    def getgrnam(name: str) -> struct_group:
        """Return the group database entry for the given group name.

        If name is not valid, raise KeyError.
        """
