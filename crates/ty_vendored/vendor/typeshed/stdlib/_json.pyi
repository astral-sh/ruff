from collections.abc import Callable
from typing import Any, final
from typing_extensions import Self

@final
class make_encoder:
    @property
    def sort_keys(self) -> bool: ...
    @property
    def skipkeys(self) -> bool: ...
    @property
    def key_separator(self) -> str: ...
    @property
    def indent(self) -> str | None: ...
    @property
    def markers(self) -> dict[int, Any] | None: ...
    @property
    def default(self) -> Callable[[Any], Any]: ...
    @property
    def encoder(self) -> Callable[[str], str]: ...
    @property
    def item_separator(self) -> str: ...
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
    def __call__(self, obj: object, _current_indent_level: int) -> Any: ...

@final
class make_scanner:
    object_hook: Any
    object_pairs_hook: Any
    parse_int: Any
    parse_constant: Any
    parse_float: Any
    strict: bool
    # TODO: 'context' needs the attrs above (ducktype), but not __call__.
    def __new__(cls, context: make_scanner) -> Self: ...
    def __call__(self, string: str, index: int) -> tuple[Any, int]: ...

def encode_basestring(s: str, /) -> str: ...
def encode_basestring_ascii(s: str, /) -> str: ...
def scanstring(string: str, end: int, strict: bool = ...) -> tuple[str, int]: ...
