"""distutils.filelist

Provides the FileList class, used for poking about the filesystem
and building lists of files.
"""

from collections.abc import Iterable
from re import Pattern
from typing import Literal, overload

# class is entirely undocumented
class FileList:
    """A list of files built by on exploring the filesystem and filtered by
    applying various patterns to what we find there.

    Instance attributes:
      dir
        directory from which files will be taken -- only used if
        'allfiles' not supplied to constructor
      files
        list of filenames currently being built/filtered/manipulated
      allfiles
        complete list of files under consideration (ie. without any
        filtering applied)
    """

    allfiles: Iterable[str] | None
    files: list[str]
    def __init__(self, warn: None = None, debug_print: None = None) -> None: ...
    def set_allfiles(self, allfiles: Iterable[str]) -> None: ...
    def findall(self, dir: str = ".") -> None: ...
    def debug_print(self, msg: str) -> None:
        """Print 'msg' to stdout if the global DEBUG (taken from the
        DISTUTILS_DEBUG environment variable) flag is true.
        """

    def append(self, item: str) -> None: ...
    def extend(self, items: Iterable[str]) -> None: ...
    def sort(self) -> None: ...
    def remove_duplicates(self) -> None: ...
    def process_template_line(self, line: str) -> None: ...
    @overload
    def include_pattern(
        self, pattern: str, anchor: bool | Literal[0, 1] = 1, prefix: str | None = None, is_regex: Literal[0, False] = 0
    ) -> bool:
        """Select strings (presumably filenames) from 'self.files' that
        match 'pattern', a Unix-style wildcard (glob) pattern.  Patterns
        are not quite the same as implemented by the 'fnmatch' module: '*'
        and '?'  match non-special characters, where "special" is platform-
        dependent: slash on Unix; colon, slash, and backslash on
        DOS/Windows; and colon on Mac OS.

        If 'anchor' is true (the default), then the pattern match is more
        stringent: "*.py" will match "foo.py" but not "foo/bar.py".  If
        'anchor' is false, both of these will match.

        If 'prefix' is supplied, then only filenames starting with 'prefix'
        (itself a pattern) and ending with 'pattern', with anything in between
        them, will match.  'anchor' is ignored in this case.

        If 'is_regex' is true, 'anchor' and 'prefix' are ignored, and
        'pattern' is assumed to be either a string containing a regex or a
        regex object -- no translation is done, the regex is just compiled
        and used as-is.

        Selected strings will be added to self.files.

        Return True if files are found, False otherwise.
        """

    @overload
    def include_pattern(self, pattern: str | Pattern[str], *, is_regex: Literal[True, 1]) -> bool: ...
    @overload
    def include_pattern(
        self,
        pattern: str | Pattern[str],
        anchor: bool | Literal[0, 1] = 1,
        prefix: str | None = None,
        is_regex: bool | Literal[0, 1] = 0,
    ) -> bool: ...
    @overload
    def exclude_pattern(
        self, pattern: str, anchor: bool | Literal[0, 1] = 1, prefix: str | None = None, is_regex: Literal[0, False] = 0
    ) -> bool:
        """Remove strings (presumably filenames) from 'files' that match
        'pattern'.  Other parameters are the same as for
        'include_pattern()', above.
        The list 'self.files' is modified in place.
        Return True if files are found, False otherwise.
        """

    @overload
    def exclude_pattern(self, pattern: str | Pattern[str], *, is_regex: Literal[True, 1]) -> bool: ...
    @overload
    def exclude_pattern(
        self,
        pattern: str | Pattern[str],
        anchor: bool | Literal[0, 1] = 1,
        prefix: str | None = None,
        is_regex: bool | Literal[0, 1] = 0,
    ) -> bool: ...

def findall(dir: str = ".") -> list[str]:
    """
    Find all files under 'dir' and return the list of full filenames.
    Unless dir is '.', return full filenames with dir prepended.
    """

def glob_to_re(pattern: str) -> str:
    """Translate a shell-like glob pattern to a regular expression; return
    a string containing the regex.  Differs from 'fnmatch.translate()' in
    that '*' does not match "special characters" (which are
    platform-specific).
    """

@overload
def translate_pattern(
    pattern: str, anchor: bool | Literal[0, 1] = 1, prefix: str | None = None, is_regex: Literal[False, 0] = 0
) -> Pattern[str]:
    """Translate a shell-like wildcard pattern to a compiled regular
    expression.  Return the compiled regex.  If 'is_regex' true,
    then 'pattern' is directly compiled to a regex (if it's a string)
    or just returned as-is (assumes it's a regex object).
    """

@overload
def translate_pattern(pattern: str | Pattern[str], *, is_regex: Literal[True, 1]) -> Pattern[str]: ...
@overload
def translate_pattern(
    pattern: str | Pattern[str], anchor: bool | Literal[0, 1] = 1, prefix: str | None = None, is_regex: bool | Literal[0, 1] = 0
) -> Pattern[str]: ...
