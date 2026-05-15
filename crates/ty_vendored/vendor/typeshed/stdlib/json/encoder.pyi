"""Implementation of JSONEncoder"""

from collections.abc import Callable, Iterator
from re import Pattern
from typing import Any, Final

ESCAPE: Final[Pattern[str]]  # undocumented
ESCAPE_ASCII: Final[Pattern[str]]  # undocumented
HAS_UTF8: Final[Pattern[bytes]]  # undocumented
ESCAPE_DCT: Final[dict[str, str]]  # undocumented
INFINITY: Final[float]  # undocumented

def py_encode_basestring(s: str) -> str:  # undocumented
    """Return a JSON representation of a Python string"""

def py_encode_basestring_ascii(s: str) -> str:  # undocumented
    """Return an ASCII-only JSON representation of a Python string"""

def encode_basestring(s: str, /) -> str:  # undocumented
    """encode_basestring(string) -> string

    Return a JSON representation of a Python string
    """

def encode_basestring_ascii(s: str, /) -> str:  # undocumented
    """encode_basestring_ascii(string) -> string

    Return an ASCII-only JSON representation of a Python string
    """

class JSONEncoder:
    """Extensible JSON <https://json.org> encoder for Python data structures.

    Supports the following objects and types by default:

    +-------------------+---------------+
    | Python            | JSON          |
    +===================+===============+
    | dict              | object        |
    +-------------------+---------------+
    | list, tuple       | array         |
    +-------------------+---------------+
    | str               | string        |
    +-------------------+---------------+
    | int, float        | number        |
    +-------------------+---------------+
    | True              | true          |
    +-------------------+---------------+
    | False             | false         |
    +-------------------+---------------+
    | None              | null          |
    +-------------------+---------------+

    To extend this to recognize other objects, subclass and implement a
    ``.default()`` method with another method that returns a serializable
    object for ``o`` if possible, otherwise it should call the superclass
    implementation (to raise ``TypeError``).

    """

    item_separator: str
    key_separator: str

    skipkeys: bool
    ensure_ascii: bool
    check_circular: bool
    allow_nan: bool
    sort_keys: bool
    indent: int | str
    def __init__(
        self,
        *,
        skipkeys: bool = False,
        ensure_ascii: bool = True,
        check_circular: bool = True,
        allow_nan: bool = True,
        sort_keys: bool = False,
        indent: int | str | None = None,
        separators: tuple[str, str] | None = None,
        default: Callable[..., Any] | None = None,
    ) -> None:
        """Constructor for JSONEncoder, with sensible defaults.

        If skipkeys is false, then it is a TypeError to attempt
        encoding of keys that are not str, int, float, bool or None.
        If skipkeys is True, such items are simply skipped.

        If ensure_ascii is true, the output is guaranteed to be str objects
        with all incoming non-ASCII and non-printable characters escaped.
        If ensure_ascii is false, the output can contain non-ASCII and
        non-printable characters.

        If check_circular is true, then lists, dicts, and custom encoded
        objects will be checked for circular references during encoding to
        prevent an infinite recursion (which would cause an RecursionError).
        Otherwise, no such check takes place.

        If allow_nan is true, then NaN, Infinity, and -Infinity will be
        encoded as such.  This behavior is not JSON specification compliant,
        but is consistent with most JavaScript based encoders and decoders.
        Otherwise, it will be a ValueError to encode such floats.

        If sort_keys is true, then the output of dictionaries will be
        sorted by key; this is useful for regression tests to ensure
        that JSON serializations can be compared on a day-to-day basis.

        If indent is a non-negative integer, then JSON array
        elements and object members will be pretty-printed with that
        indent level.  An indent level of 0 will only insert newlines.
        None is the most compact representation.

        If specified, separators should be an (item_separator,
        key_separator) tuple.  The default is (', ', ': ') if *indent* is
        ``None`` and (',', ': ') otherwise.  To get the most compact JSON
        representation, you should specify (',', ':') to eliminate
        whitespace.

        If specified, default is a function that gets called for objects
        that can't otherwise be serialized.  It should return a JSON
        encodable version of the object or raise a ``TypeError``.

        """

    def default(self, o: Any) -> Any:
        """Implement this method in a subclass such that it returns
        a serializable object for ``o``, or calls the base implementation
        (to raise a ``TypeError``).

        For example, to support arbitrary iterators, you could
        implement default like this::

            def default(self, o):
                try:
                    iterable = iter(o)
                except TypeError:
                    pass
                else:
                    return list(iterable)
                # Let the base class default method raise the TypeError
                return super().default(o)

        """

    def encode(self, o: Any) -> str:
        """Return a JSON string representation of a Python data structure.

        >>> from json.encoder import JSONEncoder
        >>> JSONEncoder().encode({"foo": ["bar", "baz"]})
        '{"foo": ["bar", "baz"]}'

        """

    def iterencode(self, o: Any, _one_shot: bool = False) -> Iterator[str]:
        """Encode the given object and yield each string
        representation as available.

        For example::

            for chunk in JSONEncoder().iterencode(bigobject):
                mysocket.write(chunk)

        """
