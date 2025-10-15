"""Shared support for scanning document type declarations in HTML and XHTML.

This module is used as a foundation for the html.parser module.  It has no
documented public API and should not be used directly.

"""

import sys
from typing import Any

class ParserBase:
    """Parser base class which provides some common support methods used
    by the SGML/HTML and XHTML parsers.
    """

    def reset(self) -> None: ...
    def getpos(self) -> tuple[int, int]:
        """Return current line number and offset."""

    def unknown_decl(self, data: str) -> None: ...
    def parse_comment(self, i: int, report: bool = True) -> int: ...  # undocumented
    def parse_declaration(self, i: int) -> int: ...  # undocumented
    def parse_marked_section(self, i: int, report: bool = True) -> int: ...  # undocumented
    def updatepos(self, i: int, j: int) -> int: ...  # undocumented
    if sys.version_info < (3, 10):
        # Removed from ParserBase: https://bugs.python.org/issue31844
        def error(self, message: str) -> Any: ...  # undocumented
    lineno: int  # undocumented
    offset: int  # undocumented
