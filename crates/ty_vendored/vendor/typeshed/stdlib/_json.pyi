"""json speedups"""

import sys
from collections.abc import Callable
from typing import Any, final
from typing_extensions import Self

@final
class make_encoder:
    """Encoder(markers, default, encoder, indent, key_separator, item_separator, sort_keys, skipkeys, allow_nan)"""

    @property
    def sort_keys(self) -> bool:
        """sort_keys"""

    @property
    def skipkeys(self) -> bool:
        """skipkeys"""

    @property
    def key_separator(self) -> str:
        """key_separator"""

    @property
    def indent(self) -> str | None:
        """indent"""

    @property
    def markers(self) -> dict[int, Any] | None:
        """markers"""

    @property
    def default(self) -> Callable[[Any], Any]:
        """default"""

    @property
    def encoder(self) -> Callable[[str], str]:
        """encoder"""

    @property
    def item_separator(self) -> str:
        """item_separator"""

    def __new__(
        cls,
        markers: dict[int, Any] | None,
        default: Callable[[Any], Any],
        encoder: Callable[[str], str],
        indent: str | None,
        key_separator: str,
        item_separator: str,
        sort_keys: bool,
        skipkeys: bool,
        allow_nan: bool,
    ) -> Self: ...
    def __call__(self, obj: object, _current_indent_level: int) -> Any:
        """Call self as a function."""

@final
class make_scanner:
    """JSON scanner object"""

    if sys.version_info >= (3, 15):
        array_hook: Any
    object_hook: Any
    """object_hook"""

    object_pairs_hook: Any
    parse_int: Any
    """parse_int"""

    parse_constant: Any
    """parse_constant"""

    parse_float: Any
    """parse_float"""

    strict: bool
    """strict"""

    # TODO: 'context' needs the attrs above (ducktype), but not __call__.
    def __new__(cls, context: make_scanner) -> Self: ...
    def __call__(self, string: str, index: int) -> tuple[Any, int]:
        """Call self as a function."""

def encode_basestring(s: str, /) -> str:
    """Return a JSON representation of a Python string"""

def encode_basestring_ascii(s: str, /) -> str:
    """Return an ASCII-only JSON representation of a Python string"""

if sys.version_info >= (3, 15):
    def scanstring(pystr: str, end: int, strict: bool = True, /) -> tuple[str, int]:
        """Scan the string s for a JSON string.

        End is the index of the character in s after the quote that started the
        JSON string. Unescapes all valid JSON string escape sequences and raises
        ValueError on attempt to decode an invalid string. If strict is False
        then literal control characters are allowed in the string.

        Returns a tuple of the decoded string and the index of the character in
        s after the end quote.
        """

else:
    def scanstring(string: str, end: int, strict: bool = True) -> tuple[str, int]:
        """scanstring(string, end, strict=True) -> (string, end)

        Scan the string s for a JSON string. End is the index of the
        character in s after the quote that started the JSON string.
        Unescapes all valid JSON string escape sequences and raises ValueError
        on attempt to decode an invalid string. If strict is False then literal
        control characters are allowed in the string.

        Returns a tuple of the decoded string and the index of the character in s
        after the end quote.
        """
