"""Support for regular expressions (RE).

This module provides regular expression matching operations similar to
those found in Perl.  It supports both 8-bit and Unicode strings; both
the pattern and the strings being processed can contain null bytes and
characters outside the US ASCII range.

Regular expressions can contain both special and ordinary characters.
Most ordinary characters, like "A", "a", or "0", are the simplest
regular expressions; they simply match themselves.  You can
concatenate ordinary characters, so last matches the string 'last'.

The special characters are:
    "."      Matches any character except a newline.
    "^"      Matches the start of the string.
    "$"      Matches the end of the string or just before the newline at
             the end of the string.
    "*"      Matches 0 or more (greedy) repetitions of the preceding RE.
             Greedy means that it will match as many repetitions as possible.
    "+"      Matches 1 or more (greedy) repetitions of the preceding RE.
    "?"      Matches 0 or 1 (greedy) of the preceding RE.
    *?,+?,?? Non-greedy versions of the previous three special characters.
    {m,n}    Matches from m to n repetitions of the preceding RE.
    {m,n}?   Non-greedy version of the above.
    "\\\\"     Either escapes special characters or signals a special sequence.
    []       Indicates a set of characters.
             A "^" as the first character indicates a complementing set.
    "|"      A|B, creates an RE that will match either A or B.
    (...)    Matches the RE inside the parentheses.
             The contents can be retrieved or matched later in the string.
    (?aiLmsux) The letters set the corresponding flags defined below.
    (?:...)  Non-grouping version of regular parentheses.
    (?P<name>...) The substring matched by the group is accessible by name.
    (?P=name)     Matches the text matched earlier by the group named name.
    (?#...)  A comment; ignored.
    (?=...)  Matches if ... matches next, but doesn't consume the string.
    (?!...)  Matches if ... doesn't match next.
    (?<=...) Matches if preceded by ... (must be fixed length).
    (?<!...) Matches if not preceded by ... (must be fixed length).
    (?(id/name)yes|no) Matches yes pattern if the group with id/name matched,
                       the (optional) no pattern otherwise.

The special sequences consist of "\\\\" and a character from the list
below.  If the ordinary character is not on the list, then the
resulting RE will match the second character.
    \\number  Matches the contents of the group of the same number.
    \\A       Matches only at the start of the string.
    \\z       Matches only at the end of the string.
    \\b       Matches the empty string, but only at the start or end of a word.
    \\B       Matches the empty string, but not at the start or end of a word.
    \\d       Matches any decimal digit; equivalent to the set [0-9] in
             bytes patterns or string patterns with the ASCII flag.
             In string patterns without the ASCII flag, it will match the whole
             range of Unicode digits.
    \\D       Matches any non-digit character; equivalent to [^\\d].
    \\s       Matches any whitespace character; equivalent to [ \\t\\n\\r\\f\\v] in
             bytes patterns or string patterns with the ASCII flag.
             In string patterns without the ASCII flag, it will match the whole
             range of Unicode whitespace characters.
    \\S       Matches any non-whitespace character; equivalent to [^\\s].
    \\w       Matches any alphanumeric character; equivalent to [a-zA-Z0-9_]
             in bytes patterns or string patterns with the ASCII flag.
             In string patterns without the ASCII flag, it will match the
             range of Unicode alphanumeric characters (letters plus digits
             plus underscore).
             With LOCALE, it will match the set [0-9_] plus characters defined
             as letters for the current locale.
    \\W       Matches the complement of \\w.
    \\\\       Matches a literal backslash.

This module exports the following functions:
    match     Match a regular expression pattern to the beginning of a string.
    fullmatch Match a regular expression pattern to all of a string.
    search    Search a string for the presence of a pattern.
    sub       Substitute occurrences of a pattern found in a string.
    subn      Same as sub, but also return the number of substitutions made.
    split     Split a string by the occurrences of a pattern.
    findall   Find all occurrences of a pattern in a string.
    finditer  Return an iterator yielding a Match object for each match.
    compile   Compile a pattern into a Pattern object.
    purge     Clear the regular expression cache.
    escape    Backslash all non-alphanumerics in a string.

Each function other than purge and escape can take an optional 'flags' argument
consisting of one or more of the following module constants, joined by "|".
A, L, and U are mutually exclusive.
    A  ASCII       For string patterns, make \\w, \\W, \\b, \\B, \\d, \\D
                   match the corresponding ASCII character categories
                   (rather than the whole Unicode categories, which is the
                   default).
                   For bytes patterns, this flag is the only available
                   behaviour and needn't be specified.
    I  IGNORECASE  Perform case-insensitive matching.
    L  LOCALE      Make \\w, \\W, \\b, \\B, dependent on the current locale.
    M  MULTILINE   "^" matches the beginning of lines (after a newline)
                   as well as the string.
                   "$" matches the end of lines (before a newline) as well
                   as the end of the string.
    S  DOTALL      "." matches any character at all, including the newline.
    X  VERBOSE     Ignore whitespace and comments for nicer looking RE's.
    U  UNICODE     For compatibility only. Ignored for string patterns (it
                   is the default), and forbidden for bytes patterns.

This module also defines exception 'PatternError', aliased to 'error' for
backward compatibility.

"""

import enum
import sre_compile
import sre_constants
import sys
from _typeshed import MaybeNone, ReadableBuffer
from collections.abc import Callable, Iterator, Mapping
from types import GenericAlias
from typing import Any, AnyStr, Final, Generic, Literal, TypeVar, final, overload
from typing_extensions import TypeAlias, deprecated

__all__ = [
    "match",
    "fullmatch",
    "search",
    "sub",
    "subn",
    "split",
    "findall",
    "finditer",
    "compile",
    "purge",
    "escape",
    "error",
    "A",
    "I",
    "L",
    "M",
    "S",
    "X",
    "U",
    "ASCII",
    "IGNORECASE",
    "LOCALE",
    "MULTILINE",
    "DOTALL",
    "VERBOSE",
    "UNICODE",
    "Match",
    "Pattern",
]
if sys.version_info < (3, 13):
    __all__ += ["template"]

if sys.version_info >= (3, 11):
    __all__ += ["NOFLAG", "RegexFlag"]

if sys.version_info >= (3, 13):
    __all__ += ["PatternError"]

    PatternError = sre_constants.error

_T = TypeVar("_T")

# The implementation defines this in re._constants (version_info >= 3, 11) or
# sre_constants. Typeshed has it here because its __module__ attribute is set to "re".
class error(Exception):
    """Exception raised for invalid regular expressions.

    Attributes:

        msg: The unformatted error message
        pattern: The regular expression pattern
        pos: The index in the pattern where compilation failed (may be None)
        lineno: The line corresponding to pos (may be None)
        colno: The column corresponding to pos (may be None)
    """

    msg: str
    pattern: str | bytes | None
    pos: int | None
    lineno: int
    colno: int
    def __init__(self, msg: str, pattern: str | bytes | None = None, pos: int | None = None) -> None: ...

@final
class Match(Generic[AnyStr]):
    """The result of re.match() and re.search().
    Match objects always have a boolean value of True.
    """

    @property
    def pos(self) -> int:
        """The index into the string at which the RE engine started looking for a match."""

    @property
    def endpos(self) -> int:
        """The index into the string beyond which the RE engine will not go."""

    @property
    def lastindex(self) -> int | None:
        """The integer index of the last matched capturing group."""

    @property
    def lastgroup(self) -> str | None:
        """The name of the last matched capturing group."""

    @property
    def string(self) -> AnyStr:
        """The string passed to match() or search()."""
    # The regular expression object whose match() or search() method produced
    # this match instance.
    @property
    def re(self) -> Pattern[AnyStr]:
        """The regular expression object."""

    @overload
    def expand(self: Match[str], template: str) -> str:
        """Return the string obtained by doing backslash substitution on the string template, as done by the sub() method."""

    @overload
    def expand(self: Match[bytes], template: ReadableBuffer) -> bytes: ...
    @overload
    def expand(self, template: AnyStr) -> AnyStr: ...
    # group() returns "AnyStr" or "AnyStr | None", depending on the pattern.
    @overload
    def group(self, group: Literal[0] = 0, /) -> AnyStr:
        """group([group1, ...]) -> str or tuple.
        Return subgroup(s) of the match by indices or names.
        For 0 returns the entire match.
        """

    @overload
    def group(self, group: str | int, /) -> AnyStr | MaybeNone: ...
    @overload
    def group(self, group1: str | int, group2: str | int, /, *groups: str | int) -> tuple[AnyStr | MaybeNone, ...]: ...
    # Each item of groups()'s return tuple is either "AnyStr" or
    # "AnyStr | None", depending on the pattern.
    @overload
    def groups(self) -> tuple[AnyStr | MaybeNone, ...]:
        """Return a tuple containing all the subgroups of the match, from 1.

        default
          Is used for groups that did not participate in the match.
        """

    @overload
    def groups(self, default: _T) -> tuple[AnyStr | _T, ...]: ...
    # Each value in groupdict()'s return dict is either "AnyStr" or
    # "AnyStr | None", depending on the pattern.
    @overload
    def groupdict(self) -> dict[str, AnyStr | MaybeNone]:
        """Return a dictionary containing all the named subgroups of the match, keyed by the subgroup name.

        default
          Is used for groups that did not participate in the match.
        """

    @overload
    def groupdict(self, default: _T) -> dict[str, AnyStr | _T]: ...
    def start(self, group: int | str = 0, /) -> int:
        """Return index of the start of the substring matched by group."""

    def end(self, group: int | str = 0, /) -> int:
        """Return index of the end of the substring matched by group."""

    def span(self, group: int | str = 0, /) -> tuple[int, int]:
        """For match object m, return the 2-tuple (m.start(group), m.end(group))."""

    @property
    def regs(self) -> tuple[tuple[int, int], ...]: ...  # undocumented
    # __getitem__() returns "AnyStr" or "AnyStr | None", depending on the pattern.
    @overload
    def __getitem__(self, key: Literal[0], /) -> AnyStr:
        """Return self[key]."""

    @overload
    def __getitem__(self, key: int | str, /) -> AnyStr | MaybeNone: ...
    def __copy__(self) -> Match[AnyStr]: ...
    def __deepcopy__(self, memo: Any, /) -> Match[AnyStr]: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

@final
class Pattern(Generic[AnyStr]):
    """Compiled regular expression object."""

    @property
    def flags(self) -> int:
        """The regex matching flags."""

    @property
    def groupindex(self) -> Mapping[str, int]:
        """A dictionary mapping group names to group numbers."""

    @property
    def groups(self) -> int:
        """The number of capturing groups in the pattern."""

    @property
    def pattern(self) -> AnyStr:
        """The pattern string from which the RE object was compiled."""

    @overload
    def search(self: Pattern[str], string: str, pos: int = 0, endpos: int = sys.maxsize) -> Match[str] | None:
        """Scan through string looking for a match, and return a corresponding match object instance.

        Return None if no position in the string matches.
        """

    @overload
    def search(self: Pattern[bytes], string: ReadableBuffer, pos: int = 0, endpos: int = sys.maxsize) -> Match[bytes] | None: ...
    @overload
    def search(self, string: AnyStr, pos: int = 0, endpos: int = sys.maxsize) -> Match[AnyStr] | None: ...
    @overload
    def match(self: Pattern[str], string: str, pos: int = 0, endpos: int = sys.maxsize) -> Match[str] | None:
        """Matches zero or more characters at the beginning of the string."""

    @overload
    def match(self: Pattern[bytes], string: ReadableBuffer, pos: int = 0, endpos: int = sys.maxsize) -> Match[bytes] | None: ...
    @overload
    def match(self, string: AnyStr, pos: int = 0, endpos: int = sys.maxsize) -> Match[AnyStr] | None: ...
    @overload
    def fullmatch(self: Pattern[str], string: str, pos: int = 0, endpos: int = sys.maxsize) -> Match[str] | None:
        """Matches against all of the string."""

    @overload
    def fullmatch(
        self: Pattern[bytes], string: ReadableBuffer, pos: int = 0, endpos: int = sys.maxsize
    ) -> Match[bytes] | None: ...
    @overload
    def fullmatch(self, string: AnyStr, pos: int = 0, endpos: int = sys.maxsize) -> Match[AnyStr] | None: ...
    @overload
    def split(self: Pattern[str], string: str, maxsplit: int = 0) -> list[str | MaybeNone]:
        """Split string by the occurrences of pattern."""

    @overload
    def split(self: Pattern[bytes], string: ReadableBuffer, maxsplit: int = 0) -> list[bytes | MaybeNone]: ...
    @overload
    def split(self, string: AnyStr, maxsplit: int = 0) -> list[AnyStr | MaybeNone]: ...
    # return type depends on the number of groups in the pattern
    @overload
    def findall(self: Pattern[str], string: str, pos: int = 0, endpos: int = sys.maxsize) -> list[Any]:
        """Return a list of all non-overlapping matches of pattern in string."""

    @overload
    def findall(self: Pattern[bytes], string: ReadableBuffer, pos: int = 0, endpos: int = sys.maxsize) -> list[Any]: ...
    @overload
    def findall(self, string: AnyStr, pos: int = 0, endpos: int = sys.maxsize) -> list[AnyStr]: ...
    @overload
    def finditer(self: Pattern[str], string: str, pos: int = 0, endpos: int = sys.maxsize) -> Iterator[Match[str]]:
        """Return an iterator over all non-overlapping matches for the RE pattern in string.

        For each match, the iterator returns a match object.
        """

    @overload
    def finditer(
        self: Pattern[bytes], string: ReadableBuffer, pos: int = 0, endpos: int = sys.maxsize
    ) -> Iterator[Match[bytes]]: ...
    @overload
    def finditer(self, string: AnyStr, pos: int = 0, endpos: int = sys.maxsize) -> Iterator[Match[AnyStr]]: ...
    @overload
    def sub(self: Pattern[str], repl: str | Callable[[Match[str]], str], string: str, count: int = 0) -> str:
        """Return the string obtained by replacing the leftmost non-overlapping occurrences of pattern in string by the replacement repl."""

    @overload
    def sub(
        self: Pattern[bytes],
        repl: ReadableBuffer | Callable[[Match[bytes]], ReadableBuffer],
        string: ReadableBuffer,
        count: int = 0,
    ) -> bytes: ...
    @overload
    def sub(self, repl: AnyStr | Callable[[Match[AnyStr]], AnyStr], string: AnyStr, count: int = 0) -> AnyStr: ...
    @overload
    def subn(self: Pattern[str], repl: str | Callable[[Match[str]], str], string: str, count: int = 0) -> tuple[str, int]:
        """Return the tuple (new_string, number_of_subs_made) found by replacing the leftmost non-overlapping occurrences of pattern with the replacement repl."""

    @overload
    def subn(
        self: Pattern[bytes],
        repl: ReadableBuffer | Callable[[Match[bytes]], ReadableBuffer],
        string: ReadableBuffer,
        count: int = 0,
    ) -> tuple[bytes, int]: ...
    @overload
    def subn(self, repl: AnyStr | Callable[[Match[AnyStr]], AnyStr], string: AnyStr, count: int = 0) -> tuple[AnyStr, int]: ...
    def __copy__(self) -> Pattern[AnyStr]: ...
    def __deepcopy__(self, memo: Any, /) -> Pattern[AnyStr]: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __hash__(self) -> int: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

# ----- re variables and constants -----

class RegexFlag(enum.IntFlag):
    """An enumeration."""

    A = sre_compile.SRE_FLAG_ASCII
    ASCII = A
    DEBUG = sre_compile.SRE_FLAG_DEBUG
    I = sre_compile.SRE_FLAG_IGNORECASE
    IGNORECASE = I
    L = sre_compile.SRE_FLAG_LOCALE
    LOCALE = L
    M = sre_compile.SRE_FLAG_MULTILINE
    MULTILINE = M
    S = sre_compile.SRE_FLAG_DOTALL
    DOTALL = S
    X = sre_compile.SRE_FLAG_VERBOSE
    VERBOSE = X
    U = sre_compile.SRE_FLAG_UNICODE
    UNICODE = U
    if sys.version_info < (3, 13):
        T = sre_compile.SRE_FLAG_TEMPLATE
        TEMPLATE = T
    if sys.version_info >= (3, 11):
        NOFLAG = 0

A: Final = RegexFlag.A
ASCII: Final = RegexFlag.ASCII
DEBUG: Final = RegexFlag.DEBUG
I: Final = RegexFlag.I
IGNORECASE: Final = RegexFlag.IGNORECASE
L: Final = RegexFlag.L
LOCALE: Final = RegexFlag.LOCALE
M: Final = RegexFlag.M
MULTILINE: Final = RegexFlag.MULTILINE
S: Final = RegexFlag.S
DOTALL: Final = RegexFlag.DOTALL
X: Final = RegexFlag.X
VERBOSE: Final = RegexFlag.VERBOSE
U: Final = RegexFlag.U
UNICODE: Final = RegexFlag.UNICODE
if sys.version_info < (3, 13):
    T: Final = RegexFlag.T
    TEMPLATE: Final = RegexFlag.TEMPLATE
if sys.version_info >= (3, 11):
    NOFLAG: Final = RegexFlag.NOFLAG
_FlagsType: TypeAlias = int | RegexFlag

# Type-wise the compile() overloads are unnecessary, they could also be modeled using
# unions in the parameter types. However mypy has a bug regarding TypeVar
# constraints (https://github.com/python/mypy/issues/11880),
# which limits us here because AnyStr is a constrained TypeVar.

# pattern arguments do *not* accept arbitrary buffers such as bytearray,
# because the pattern must be hashable.
@overload
def compile(pattern: AnyStr, flags: _FlagsType = 0) -> Pattern[AnyStr]:
    """Compile a regular expression pattern, returning a Pattern object."""

@overload
def compile(pattern: Pattern[AnyStr], flags: _FlagsType = 0) -> Pattern[AnyStr]: ...
@overload
def search(pattern: str | Pattern[str], string: str, flags: _FlagsType = 0) -> Match[str] | None:
    """Scan through string looking for a match to the pattern, returning
    a Match object, or None if no match was found.
    """

@overload
def search(pattern: bytes | Pattern[bytes], string: ReadableBuffer, flags: _FlagsType = 0) -> Match[bytes] | None: ...
@overload
def match(pattern: str | Pattern[str], string: str, flags: _FlagsType = 0) -> Match[str] | None:
    """Try to apply the pattern at the start of the string, returning
    a Match object, or None if no match was found.
    """

@overload
def match(pattern: bytes | Pattern[bytes], string: ReadableBuffer, flags: _FlagsType = 0) -> Match[bytes] | None: ...
@overload
def fullmatch(pattern: str | Pattern[str], string: str, flags: _FlagsType = 0) -> Match[str] | None:
    """Try to apply the pattern to all of the string, returning
    a Match object, or None if no match was found.
    """

@overload
def fullmatch(pattern: bytes | Pattern[bytes], string: ReadableBuffer, flags: _FlagsType = 0) -> Match[bytes] | None: ...
@overload
def split(pattern: str | Pattern[str], string: str, maxsplit: int = 0, flags: _FlagsType = 0) -> list[str | MaybeNone]:
    """Split the source string by the occurrences of the pattern,
    returning a list containing the resulting substrings.  If
    capturing parentheses are used in pattern, then the text of all
    groups in the pattern are also returned as part of the resulting
    list.  If maxsplit is nonzero, at most maxsplit splits occur,
    and the remainder of the string is returned as the final element
    of the list.
    """

@overload
def split(
    pattern: bytes | Pattern[bytes], string: ReadableBuffer, maxsplit: int = 0, flags: _FlagsType = 0
) -> list[bytes | MaybeNone]: ...
@overload
def findall(pattern: str | Pattern[str], string: str, flags: _FlagsType = 0) -> list[Any]:
    """Return a list of all non-overlapping matches in the string.

    If one or more capturing groups are present in the pattern, return
    a list of groups; this will be a list of tuples if the pattern
    has more than one group.

    Empty matches are included in the result.
    """

@overload
def findall(pattern: bytes | Pattern[bytes], string: ReadableBuffer, flags: _FlagsType = 0) -> list[Any]: ...
@overload
def finditer(pattern: str | Pattern[str], string: str, flags: _FlagsType = 0) -> Iterator[Match[str]]:
    """Return an iterator over all non-overlapping matches in the
    string.  For each match, the iterator returns a Match object.

    Empty matches are included in the result.
    """

@overload
def finditer(pattern: bytes | Pattern[bytes], string: ReadableBuffer, flags: _FlagsType = 0) -> Iterator[Match[bytes]]: ...
@overload
def sub(
    pattern: str | Pattern[str], repl: str | Callable[[Match[str]], str], string: str, count: int = 0, flags: _FlagsType = 0
) -> str:
    """Return the string obtained by replacing the leftmost
    non-overlapping occurrences of the pattern in string by the
    replacement repl.  repl can be either a string or a callable;
    if a string, backslash escapes in it are processed.  If it is
    a callable, it's passed the Match object and must return
    a replacement string to be used.
    """

@overload
def sub(
    pattern: bytes | Pattern[bytes],
    repl: ReadableBuffer | Callable[[Match[bytes]], ReadableBuffer],
    string: ReadableBuffer,
    count: int = 0,
    flags: _FlagsType = 0,
) -> bytes: ...
@overload
def subn(
    pattern: str | Pattern[str], repl: str | Callable[[Match[str]], str], string: str, count: int = 0, flags: _FlagsType = 0
) -> tuple[str, int]:
    """Return a 2-tuple containing (new_string, number).
    new_string is the string obtained by replacing the leftmost
    non-overlapping occurrences of the pattern in the source
    string by the replacement repl.  number is the number of
    substitutions that were made. repl can be either a string or a
    callable; if a string, backslash escapes in it are processed.
    If it is a callable, it's passed the Match object and must
    return a replacement string to be used.
    """

@overload
def subn(
    pattern: bytes | Pattern[bytes],
    repl: ReadableBuffer | Callable[[Match[bytes]], ReadableBuffer],
    string: ReadableBuffer,
    count: int = 0,
    flags: _FlagsType = 0,
) -> tuple[bytes, int]: ...
def escape(pattern: AnyStr) -> AnyStr:
    """
    Escape special characters in a string.
    """

def purge() -> None:
    """Clear the regular expression caches"""

if sys.version_info < (3, 13):
    if sys.version_info >= (3, 11):
        @deprecated("Deprecated since Python 3.11; removed in Python 3.13. Use `re.compile()` instead.")
        def template(pattern: AnyStr | Pattern[AnyStr], flags: _FlagsType = 0) -> Pattern[AnyStr]:  # undocumented
            """Compile a template pattern, returning a Pattern object, deprecated"""
    else:
        def template(pattern: AnyStr | Pattern[AnyStr], flags: _FlagsType = 0) -> Pattern[AnyStr]:  # undocumented
            """Compile a template pattern, returning a Pattern object"""
