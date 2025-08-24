"""distutils.fancy_getopt

Wrapper around the standard getopt module that provides the following
additional features:
  * short and long options are tied together
  * options have help strings, so fancy_getopt could potentially
    create a complete usage summary
  * options set attributes of a passed-in object
"""

from collections.abc import Iterable, Mapping
from getopt import _SliceableT, _StrSequenceT_co
from re import Pattern
from typing import Any, Final, overload
from typing_extensions import TypeAlias

_Option: TypeAlias = tuple[str, str | None, str]

longopt_pat: Final = r"[a-zA-Z](?:[a-zA-Z0-9-]*)"
longopt_re: Final[Pattern[str]]
neg_alias_re: Final[Pattern[str]]
longopt_xlate: Final[dict[int, int]]

class FancyGetopt:
    """Wrapper around the standard 'getopt()' module that provides some
    handy extra functionality:
      * short and long options are tied together
      * options have help strings, and help text can be assembled
        from them
      * options set attributes of a passed-in object
      * boolean options can have "negative aliases" -- eg. if
        --quiet is the "negative alias" of --verbose, then "--quiet"
        on the command line sets 'verbose' to false
    """

    def __init__(self, option_table: list[_Option] | None = None) -> None: ...
    # TODO: kinda wrong, `getopt(object=object())` is invalid
    @overload
    def getopt(
        self, args: _SliceableT[_StrSequenceT_co] | None = None, object: None = None
    ) -> tuple[_StrSequenceT_co, OptionDummy]:
        """Parse command-line options in args. Store as attributes on object.

        If 'args' is None or not supplied, uses 'sys.argv[1:]'.  If
        'object' is None or not supplied, creates a new OptionDummy
        object, stores option values there, and returns a tuple (args,
        object).  If 'object' is supplied, it is modified in place and
        'getopt()' just returns 'args'; in both cases, the returned
        'args' is a modified copy of the passed-in 'args' list, which
        is left untouched.
        """

    @overload
    def getopt(
        self, args: _SliceableT[_StrSequenceT_co] | None, object: Any
    ) -> _StrSequenceT_co: ...  # object is an arbitrary non-slotted object
    def get_option_order(self) -> list[tuple[str, str]]:
        """Returns the list of (option, value) tuples processed by the
        previous run of 'getopt()'.  Raises RuntimeError if
        'getopt()' hasn't been called yet.
        """

    def generate_help(self, header: str | None = None) -> list[str]:
        """Generate help text (a list of strings, one per suggested line of
        output) from the option table for this FancyGetopt object.
        """

# Same note as FancyGetopt.getopt
@overload
def fancy_getopt(
    options: list[_Option], negative_opt: Mapping[_Option, _Option], object: None, args: _SliceableT[_StrSequenceT_co] | None
) -> tuple[_StrSequenceT_co, OptionDummy]: ...
@overload
def fancy_getopt(
    options: list[_Option], negative_opt: Mapping[_Option, _Option], object: Any, args: _SliceableT[_StrSequenceT_co] | None
) -> _StrSequenceT_co: ...

WS_TRANS: Final[dict[int, str]]

def wrap_text(text: str, width: int) -> list[str]:
    """wrap_text(text : string, width : int) -> [string]

    Split 'text' into multiple lines of no more than 'width' characters
    each, and return the list of strings that results.
    """

def translate_longopt(opt: str) -> str:
    """Convert a long option name to a valid Python identifier by
    changing "-" to "_".
    """

class OptionDummy:
    """Dummy class just used as a place to hold command-line option
    values as instance attributes.
    """

    def __init__(self, options: Iterable[str] = []) -> None:
        """Create a new OptionDummy instance.  The attributes listed in
        'options' will be initialized to None.
        """
