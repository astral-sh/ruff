import builtins
import sys
import types
from _typeshed import ReadableBuffer, SupportsRead, SupportsWrite
from typing import Any, Final
from typing_extensions import TypeAlias

version: Final[int]

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

if sys.version_info >= (3, 14):
    def dump(value: _Marshallable, file: SupportsWrite[bytes], version: int = 5, /, *, allow_code: bool = True) -> None: ...
    def dumps(value: _Marshallable, version: int = 5, /, *, allow_code: bool = True) -> bytes: ...

elif sys.version_info >= (3, 13):
    def dump(value: _Marshallable, file: SupportsWrite[bytes], version: int = 4, /, *, allow_code: bool = True) -> None: ...
    def dumps(value: _Marshallable, version: int = 4, /, *, allow_code: bool = True) -> bytes: ...

else:
    def dump(value: _Marshallable, file: SupportsWrite[bytes], version: int = 4, /) -> None: ...
    def dumps(value: _Marshallable, version: int = 4, /) -> bytes: ...

if sys.version_info >= (3, 13):
    def load(file: SupportsRead[bytes], /, *, allow_code: bool = True) -> Any: ...
    def loads(bytes: ReadableBuffer, /, *, allow_code: bool = True) -> Any: ...

else:
    def load(file: SupportsRead[bytes], /) -> Any: ...
    def loads(bytes: ReadableBuffer, /) -> Any: ...
