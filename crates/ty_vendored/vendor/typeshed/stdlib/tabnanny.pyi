"""The Tab Nanny despises ambiguous indentation.  She knows no mercy.

tabnanny -- Detection of ambiguous indentation

For the time being this module is intended to be called as a script.
However it is possible to import it into an IDE and use the function
check() described below.

Warning: The API provided by this module is likely to change in future
releases; such changes may not be backward compatible.
"""

from _typeshed import StrOrBytesPath
from collections.abc import Iterable

__all__ = ["check", "NannyNag", "process_tokens"]

verbose: int
filename_only: int

class NannyNag(Exception):
    """
    Raised by process_tokens() if detecting an ambiguous indent.
    Captured and handled in check().
    """

    def __init__(self, lineno: int, msg: str, line: str) -> None: ...
    def get_lineno(self) -> int: ...
    def get_msg(self) -> str: ...
    def get_line(self) -> str: ...

def check(file: StrOrBytesPath) -> None:
    """check(file_or_dir)

    If file_or_dir is a directory and not a symbolic link, then recursively
    descend the directory tree named by file_or_dir, checking all .py files
    along the way. If file_or_dir is an ordinary Python source file, it is
    checked for whitespace related problems. The diagnostic messages are
    written to standard output using the print statement.
    """

def process_tokens(tokens: Iterable[tuple[int, str, tuple[int, int], tuple[int, int], str]]) -> None: ...
