from _pickle import (
    PickleError as PickleError,
    Pickler as Pickler,
    PicklingError as PicklingError,
    Unpickler as Unpickler,
    UnpicklingError as UnpicklingError,
    _BufferCallback,
    _ReadableFileobj,
    _ReducedType,
    dump as dump,
    dumps as dumps,
    load as load,
    loads as loads,
)
from _typeshed import ReadableBuffer, SupportsWrite
from collections.abc import Callable, Iterable, Mapping
from typing import Any, ClassVar, Final, SupportsBytes, SupportsIndex, final
from typing_extensions import Self

__all__ = [
    "PickleBuffer",
    "PickleError",
    "PicklingError",
    "UnpicklingError",
    "Pickler",
    "Unpickler",
    "dump",
    "dumps",
    "load",
    "loads",
    "ADDITEMS",
    "APPEND",
    "APPENDS",
    "BINBYTES",
    "BINBYTES8",
    "BINFLOAT",
    "BINGET",
    "BININT",
    "BININT1",
    "BININT2",
    "BINPERSID",
    "BINPUT",
    "BINSTRING",
    "BINUNICODE",
    "BINUNICODE8",
    "BUILD",
    "BYTEARRAY8",
    "DEFAULT_PROTOCOL",
    "DICT",
    "DUP",
    "EMPTY_DICT",
    "EMPTY_LIST",
    "EMPTY_SET",
    "EMPTY_TUPLE",
    "EXT1",
    "EXT2",
    "EXT4",
    "FALSE",
    "FLOAT",
    "FRAME",
    "FROZENSET",
    "GET",
    "GLOBAL",
    "HIGHEST_PROTOCOL",
    "INST",
    "INT",
    "LIST",
    "LONG",
    "LONG1",
    "LONG4",
    "LONG_BINGET",
    "LONG_BINPUT",
    "MARK",
    "MEMOIZE",
    "NEWFALSE",
    "NEWOBJ",
    "NEWOBJ_EX",
    "NEWTRUE",
    "NEXT_BUFFER",
    "NONE",
    "OBJ",
    "PERSID",
    "POP",
    "POP_MARK",
    "PROTO",
    "PUT",
    "READONLY_BUFFER",
    "REDUCE",
    "SETITEM",
    "SETITEMS",
    "SHORT_BINBYTES",
    "SHORT_BINSTRING",
    "SHORT_BINUNICODE",
    "STACK_GLOBAL",
    "STOP",
    "STRING",
    "TRUE",
    "TUPLE",
    "TUPLE1",
    "TUPLE2",
    "TUPLE3",
    "UNICODE",
]

HIGHEST_PROTOCOL: Final = 5
DEFAULT_PROTOCOL: Final = 5

bytes_types: tuple[type[Any], ...]  # undocumented

@final
class PickleBuffer:
    def __new__(cls, buffer: ReadableBuffer) -> Self: ...
    def raw(self) -> memoryview: ...
    def release(self) -> None: ...
    def __buffer__(self, flags: int, /) -> memoryview: ...
    def __release_buffer__(self, buffer: memoryview, /) -> None: ...

MARK: Final = b"("
STOP: Final = b"."
POP: Final = b"0"
POP_MARK: Final = b"1"
DUP: Final = b"2"
FLOAT: Final = b"F"
INT: Final = b"I"
BININT: Final = b"J"
BININT1: Final = b"K"
LONG: Final = b"L"
BININT2: Final = b"M"
NONE: Final = b"N"
PERSID: Final = b"P"
BINPERSID: Final = b"Q"
REDUCE: Final = b"R"
STRING: Final = b"S"
BINSTRING: Final = b"T"
SHORT_BINSTRING: Final = b"U"
UNICODE: Final = b"V"
BINUNICODE: Final = b"X"
APPEND: Final = b"a"
BUILD: Final = b"b"
GLOBAL: Final = b"c"
DICT: Final = b"d"
EMPTY_DICT: Final = b"}"
APPENDS: Final = b"e"
GET: Final = b"g"
BINGET: Final = b"h"
INST: Final = b"i"
LONG_BINGET: Final = b"j"
LIST: Final = b"l"
EMPTY_LIST: Final = b"]"
OBJ: Final = b"o"
PUT: Final = b"p"
BINPUT: Final = b"q"
LONG_BINPUT: Final = b"r"
SETITEM: Final = b"s"
TUPLE: Final = b"t"
EMPTY_TUPLE: Final = b")"
SETITEMS: Final = b"u"
BINFLOAT: Final = b"G"

TRUE: Final = b"I01\n"
FALSE: Final = b"I00\n"

# protocol 2
PROTO: Final = b"\x80"
NEWOBJ: Final = b"\x81"
EXT1: Final = b"\x82"
EXT2: Final = b"\x83"
EXT4: Final = b"\x84"
TUPLE1: Final = b"\x85"
TUPLE2: Final = b"\x86"
TUPLE3: Final = b"\x87"
NEWTRUE: Final = b"\x88"
NEWFALSE: Final = b"\x89"
LONG1: Final = b"\x8a"
LONG4: Final = b"\x8b"

# protocol 3
BINBYTES: Final = b"B"
SHORT_BINBYTES: Final = b"C"

# protocol 4
SHORT_BINUNICODE: Final = b"\x8c"
BINUNICODE8: Final = b"\x8d"
BINBYTES8: Final = b"\x8e"
EMPTY_SET: Final = b"\x8f"
ADDITEMS: Final = b"\x90"
FROZENSET: Final = b"\x91"
NEWOBJ_EX: Final = b"\x92"
STACK_GLOBAL: Final = b"\x93"
MEMOIZE: Final = b"\x94"
FRAME: Final = b"\x95"

# protocol 5
BYTEARRAY8: Final = b"\x96"
NEXT_BUFFER: Final = b"\x97"
READONLY_BUFFER: Final = b"\x98"

def encode_long(x: int) -> bytes: ...  # undocumented
def decode_long(data: Iterable[SupportsIndex] | SupportsBytes | ReadableBuffer) -> int: ...  # undocumented

# undocumented pure-Python implementations
class _Pickler:
    fast: bool
    dispatch_table: Mapping[type, Callable[[Any], _ReducedType]]
    bin: bool  # undocumented
    dispatch: ClassVar[dict[type, Callable[[Unpickler, Any], None]]]  # undocumented, _Pickler only
    reducer_override: Callable[[Any], Any]
    def __init__(
        self,
        file: SupportsWrite[bytes],
        protocol: int | None = None,
        *,
        fix_imports: bool = True,
        buffer_callback: _BufferCallback = None,
    ) -> None: ...
    def dump(self, obj: Any) -> None: ...
    def clear_memo(self) -> None: ...
    def persistent_id(self, obj: Any) -> Any: ...

class _Unpickler:
    dispatch: ClassVar[dict[int, Callable[[Unpickler], None]]]  # undocumented, _Unpickler only
    def __init__(
        self,
        file: _ReadableFileobj,
        *,
        fix_imports: bool = True,
        encoding: str = "ASCII",
        errors: str = "strict",
        buffers: Iterable[Any] | None = None,
    ) -> None: ...
    def load(self) -> Any: ...
    def find_class(self, module: str, name: str) -> Any: ...
    def persistent_load(self, pid: Any) -> Any: ...
