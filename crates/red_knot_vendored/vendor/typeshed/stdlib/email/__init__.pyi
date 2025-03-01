from collections.abc import Callable
from email.message import Message
from email.policy import Policy, _MessageT
from typing import IO, overload
from typing_extensions import TypeAlias

# At runtime, listing submodules in __all__ without them being imported is
# valid, and causes them to be included in a star import. See #6523

__all__ = [  # noqa: F822  # Undefined names in __all__
    "base64mime",  # pyright: ignore[reportUnsupportedDunderAll]
    "charset",  # pyright: ignore[reportUnsupportedDunderAll]
    "encoders",  # pyright: ignore[reportUnsupportedDunderAll]
    "errors",  # pyright: ignore[reportUnsupportedDunderAll]
    "feedparser",  # pyright: ignore[reportUnsupportedDunderAll]
    "generator",  # pyright: ignore[reportUnsupportedDunderAll]
    "header",  # pyright: ignore[reportUnsupportedDunderAll]
    "iterators",  # pyright: ignore[reportUnsupportedDunderAll]
    "message",  # pyright: ignore[reportUnsupportedDunderAll]
    "message_from_file",
    "message_from_binary_file",
    "message_from_string",
    "message_from_bytes",
    "mime",  # pyright: ignore[reportUnsupportedDunderAll]
    "parser",  # pyright: ignore[reportUnsupportedDunderAll]
    "quoprimime",  # pyright: ignore[reportUnsupportedDunderAll]
    "utils",  # pyright: ignore[reportUnsupportedDunderAll]
]

# Definitions imported by multiple submodules in typeshed
_ParamType: TypeAlias = str | tuple[str | None, str | None, str]  # noqa: Y047
_ParamsType: TypeAlias = str | None | tuple[str, str | None, str]  # noqa: Y047

@overload
def message_from_string(s: str) -> Message: ...
@overload
def message_from_string(s: str, _class: Callable[[], _MessageT]) -> _MessageT: ...
@overload
def message_from_string(s: str, _class: Callable[[], _MessageT] = ..., *, policy: Policy[_MessageT]) -> _MessageT: ...
@overload
def message_from_bytes(s: bytes | bytearray) -> Message: ...
@overload
def message_from_bytes(s: bytes | bytearray, _class: Callable[[], _MessageT]) -> _MessageT: ...
@overload
def message_from_bytes(
    s: bytes | bytearray, _class: Callable[[], _MessageT] = ..., *, policy: Policy[_MessageT]
) -> _MessageT: ...
@overload
def message_from_file(fp: IO[str]) -> Message: ...
@overload
def message_from_file(fp: IO[str], _class: Callable[[], _MessageT]) -> _MessageT: ...
@overload
def message_from_file(fp: IO[str], _class: Callable[[], _MessageT] = ..., *, policy: Policy[_MessageT]) -> _MessageT: ...
@overload
def message_from_binary_file(fp: IO[bytes]) -> Message: ...
@overload
def message_from_binary_file(fp: IO[bytes], _class: Callable[[], _MessageT]) -> _MessageT: ...
@overload
def message_from_binary_file(fp: IO[bytes], _class: Callable[[], _MessageT] = ..., *, policy: Policy[_MessageT]) -> _MessageT: ...
