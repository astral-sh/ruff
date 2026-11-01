"""Parser for command line options.

This module helps scripts to parse the command line arguments in
sys.argv.  It supports the same conventions as the Unix getopt()
function (including the special meanings of arguments of the form '-'
and '--').  Long options similar to those supported by GNU software
may be used as well via an optional third argument.  This module
provides two functions and an exception:

getopt() -- Parse command line options
gnu_getopt() -- Like getopt(), but allow option and non-option arguments
to be intermixed.
GetoptError -- exception (class) raised with 'opt' attribute, which is the
option involved with the exception.
"""

from collections.abc import Iterable, Sequence
from typing import Protocol, TypeVar, overload, type_check_only

_StrSequenceT_co = TypeVar("_StrSequenceT_co", covariant=True, bound=Sequence[str])

@type_check_only
class _SliceableT(Protocol[_StrSequenceT_co]):
    @overload
    def __getitem__(self, key: int, /) -> str: ...
    @overload
    def __getitem__(self, key: slice, /) -> _StrSequenceT_co: ...

__all__ = ["GetoptError", "error", "getopt", "gnu_getopt"]

def getopt(
    args: _SliceableT[_StrSequenceT_co], shortopts: str, longopts: Iterable[str] | str = []
) -> tuple[list[tuple[str, str]], _StrSequenceT_co]:
    """getopt(args, options[, long_options]) -> opts, args

    Parses command line options and parameter list.  args is the
    argument list to be parsed, without the leading reference to the
    running program.  Typically, this means "sys.argv[1:]".  shortopts
    is the string of option letters that the script wants to
    recognize, with options that require an argument followed by a
    colon and options that accept an optional argument followed by
    two colons (i.e., the same format that Unix getopt() uses).  If
    specified, longopts is a list of strings with the names of the
    long options which should be supported.  The leading '--'
    characters should not be included in the option name.  Options
    which require an argument should be followed by an equal sign
    ('=').  Options which accept an optional argument should be
    followed by an equal sign and question mark ('=?').

    The return value consists of two elements: the first is a list of
    (option, value) pairs; the second is the list of program arguments
    left after the option list was stripped (this is a trailing slice
    of the first argument).  Each option-and-value pair returned has
    the option as its first element, prefixed with a hyphen (e.g.,
    '-x'), and the option argument as its second element, or an empty
    string if the option has no argument.  The options occur in the
    list in the same order in which they were found, thus allowing
    multiple occurrences.  Long and short options may be mixed.

    """

def gnu_getopt(
    args: Sequence[str], shortopts: str, longopts: Iterable[str] | str = []
) -> tuple[list[tuple[str, str]], list[str]]:
    """getopt(args, options[, long_options]) -> opts, args

    This function works like getopt(), except that GNU style scanning
    mode is used by default. This means that option and non-option
    arguments may be intermixed. The getopt() function stops
    processing options as soon as a non-option argument is
    encountered.

    If the first character of the option string is '+', or if the
    environment variable POSIXLY_CORRECT is set, then option
    processing stops as soon as a non-option argument is encountered.

    """

class GetoptError(Exception):
    msg: str
    opt: str
    def __init__(self, msg: str, opt: str = "") -> None: ...

error = GetoptError
