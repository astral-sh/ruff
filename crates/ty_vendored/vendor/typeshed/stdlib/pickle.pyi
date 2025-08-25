"""Create portable serialized representations of Python objects.

See module copyreg for a mechanism for registering custom picklers.
See module pickletools source for extensive comments.

Classes:

    Pickler
    Unpickler

Functions:

    dump(object, file)
    dumps(object) -> string
    load(file) -> object
    loads(bytes) -> object

Misc variables:

    __version__
    format_version
    compatible_formats

"""

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
    """Wrapper for potentially out-of-band buffers"""

    def __new__(cls, buffer: ReadableBuffer) -> Self: ...
    def raw(self) -> memoryview:
        """Return a memoryview of the raw memory underlying this buffer.
        Will raise BufferError is the buffer isn't contiguous.
        """

    def release(self) -> None:
        """Release the underlying buffer exposed by the PickleBuffer object."""

    def __buffer__(self, flags: int, /) -> memoryview:
        """Return a buffer object that exposes the underlying memory of the object."""

    def __release_buffer__(self, buffer: memoryview, /) -> None:
        """Release the buffer object that exposes the underlying memory of the object."""

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

def encode_long(x: int) -> bytes:  # undocumented
    """Encode a long to a two's complement little-endian binary string.
    Note that 0 is a special case, returning an empty string, to save a
    byte in the LONG1 pickling context.

    >>> encode_long(0)
    b''
    >>> encode_long(255)
    b'\\xff\\x00'
    >>> encode_long(32767)
    b'\\xff\\x7f'
    >>> encode_long(-256)
    b'\\x00\\xff'
    >>> encode_long(-32768)
    b'\\x00\\x80'
    >>> encode_long(-128)
    b'\\x80'
    >>> encode_long(127)
    b'\\x7f'
    >>>
    """

def decode_long(data: Iterable[SupportsIndex] | SupportsBytes | ReadableBuffer) -> int:  # undocumented
    """Decode a long from a two's complement little-endian binary string.

    >>> decode_long(b'')
    0
    >>> decode_long(b"\\xff\\x00")
    255
    >>> decode_long(b"\\xff\\x7f")
    32767
    >>> decode_long(b"\\x00\\xff")
    -256
    >>> decode_long(b"\\x00\\x80")
    -32768
    >>> decode_long(b"\\x80")
    -128
    >>> decode_long(b"\\x7f")
    127
    """

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
    ) -> None:
        """This takes a binary file for writing a pickle data stream.

        The optional *protocol* argument tells the pickler to use the
        given protocol; supported protocols are 0, 1, 2, 3, 4 and 5.
        The default protocol is 5. It was introduced in Python 3.8, and
        is incompatible with previous versions.

        Specifying a negative protocol version selects the highest
        protocol version supported.  The higher the protocol used, the
        more recent the version of Python needed to read the pickle
        produced.

        The *file* argument must have a write() method that accepts a
        single bytes argument. It can thus be a file object opened for
        binary writing, an io.BytesIO instance, or any other custom
        object that meets this interface.

        If *fix_imports* is True and *protocol* is less than 3, pickle
        will try to map the new Python 3 names to the old module names
        used in Python 2, so that the pickle data stream is readable
        with Python 2.

        If *buffer_callback* is None (the default), buffer views are
        serialized into *file* as part of the pickle stream.

        If *buffer_callback* is not None, then it can be called any number
        of times with a buffer view.  If the callback returns a false value
        (such as None), the given buffer is out-of-band; otherwise the
        buffer is serialized in-band, i.e. inside the pickle stream.

        It is an error if *buffer_callback* is not None and *protocol*
        is None or smaller than 5.
        """

    def dump(self, obj: Any) -> None:
        """Write a pickled representation of obj to the open file."""

    def clear_memo(self) -> None:
        """Clears the pickler's "memo".

        The memo is the data structure that remembers which objects the
        pickler has already seen, so that shared or recursive objects
        are pickled by reference and not by value.  This method is
        useful when re-using picklers.
        """

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
    ) -> None:
        """This takes a binary file for reading a pickle data stream.

        The protocol version of the pickle is detected automatically, so
        no proto argument is needed.

        The argument *file* must have two methods, a read() method that
        takes an integer argument, and a readline() method that requires
        no arguments.  Both methods should return bytes.  Thus *file*
        can be a binary file object opened for reading, an io.BytesIO
        object, or any other custom object that meets this interface.

        The file-like object must have two methods, a read() method
        that takes an integer argument, and a readline() method that
        requires no arguments.  Both methods should return bytes.
        Thus file-like object can be a binary file object opened for
        reading, a BytesIO object, or any other custom object that
        meets this interface.

        If *buffers* is not None, it should be an iterable of buffer-enabled
        objects that is consumed each time the pickle stream references
        an out-of-band buffer view.  Such buffers have been given in order
        to the *buffer_callback* of a Pickler object.

        If *buffers* is None (the default), then the buffers are taken
        from the pickle stream, assuming they are serialized there.
        It is an error for *buffers* to be None if the pickle stream
        was produced with a non-None *buffer_callback*.

        Other optional arguments are *fix_imports*, *encoding* and
        *errors*, which are used to control compatibility support for
        pickle stream generated by Python 2.  If *fix_imports* is True,
        pickle will try to map the old Python 2 names to the new names
        used in Python 3.  The *encoding* and *errors* tell pickle how
        to decode 8-bit string instances pickled by Python 2; these
        default to 'ASCII' and 'strict', respectively. *encoding* can be
        'bytes' to read these 8-bit string instances as bytes objects.
        """

    def load(self) -> Any:
        """Read a pickled object representation from the open file.

        Return the reconstituted object hierarchy specified in the file.
        """

    def find_class(self, module: str, name: str) -> Any: ...
    def persistent_load(self, pid: Any) -> Any: ...
