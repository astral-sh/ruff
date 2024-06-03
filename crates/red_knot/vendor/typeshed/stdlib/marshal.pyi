import builtins
import sys
import types
from _typeshed import ReadableBuffer, SupportsRead, SupportsWrite
from typing import Any
from typing_extensions import TypeAlias

version: int

_Marshallable: TypeAlias = (
    # handled in w_object() in marshal.c
    None
    | type[StopIteration]
    | builtins.ellipsis
    | bool
    # handled in w_complex_object() in marshal.c
    | int
    | float
    | complex
    | bytes
    | str
    | tuple[_Marshallable, ...]
    | list[Any]
    | dict[Any, Any]
    | set[Any]
    | frozenset[_Marshallable]
    | types.CodeType
    | ReadableBuffer
)

if sys.version_info >= (3, 13):
    def dump(value: _Marshallable, file: SupportsWrite[bytes], version: int = 4, /, *, allow_code: bool = True) -> None: ...
    def load(file: SupportsRead[bytes], /, *, allow_code: bool = True) -> Any: ...
    def dumps(value: _Marshallable, version: int = 4, /, *, allow_code: bool = True) -> bytes: ...
    def loads(bytes: ReadableBuffer, /, *, allow_code: bool = True) -> Any: ...

else:
    def dump(value: _Marshallable, file: SupportsWrite[bytes], version: int = 4, /) -> None: ...
    def load(file: SupportsRead[bytes], /) -> Any: ...
    def dumps(value: _Marshallable, version: int = 4, /) -> bytes: ...
    def loads(bytes: ReadableBuffer, /) -> Any: ...
