""" "Executable documentation" for the pickle module.

Extensive comments about the pickle protocols and pickle-machine opcodes
can be found here.  Some functions meant for external use:

genops(pickle)
   Generate all the opcodes in a pickle, as (opcode, arg, position) triples.

dis(pickle, out=None, memo=None, indentlevel=4)
   Print a symbolic disassembly of a pickle.
"""

import sys
from collections.abc import Callable, Iterator, MutableMapping
from typing import IO, Any, Final
from typing_extensions import TypeAlias

__all__ = ["dis", "genops", "optimize"]

_Reader: TypeAlias = Callable[[IO[bytes]], Any]
bytes_types: tuple[type[Any], ...]

UP_TO_NEWLINE: Final = -1
TAKEN_FROM_ARGUMENT1: Final = -2
TAKEN_FROM_ARGUMENT4: Final = -3
TAKEN_FROM_ARGUMENT4U: Final = -4
TAKEN_FROM_ARGUMENT8U: Final = -5

class ArgumentDescriptor:
    __slots__ = ("name", "n", "reader", "doc")
    name: str
    n: int
    reader: _Reader
    doc: str
    def __init__(self, name: str, n: int, reader: _Reader, doc: str) -> None: ...

def read_uint1(f: IO[bytes]) -> int:
    """
    >>> import io
    >>> read_uint1(io.BytesIO(b'\\xff'))
    255
    """

uint1: ArgumentDescriptor

def read_uint2(f: IO[bytes]) -> int:
    """
    >>> import io
    >>> read_uint2(io.BytesIO(b'\\xff\\x00'))
    255
    >>> read_uint2(io.BytesIO(b'\\xff\\xff'))
    65535
    """

uint2: ArgumentDescriptor

def read_int4(f: IO[bytes]) -> int:
    """
    >>> import io
    >>> read_int4(io.BytesIO(b'\\xff\\x00\\x00\\x00'))
    255
    >>> read_int4(io.BytesIO(b'\\x00\\x00\\x00\\x80')) == -(2**31)
    True
    """

int4: ArgumentDescriptor

def read_uint4(f: IO[bytes]) -> int:
    """
    >>> import io
    >>> read_uint4(io.BytesIO(b'\\xff\\x00\\x00\\x00'))
    255
    >>> read_uint4(io.BytesIO(b'\\x00\\x00\\x00\\x80')) == 2**31
    True
    """

uint4: ArgumentDescriptor

def read_uint8(f: IO[bytes]) -> int:
    """
    >>> import io
    >>> read_uint8(io.BytesIO(b'\\xff\\x00\\x00\\x00\\x00\\x00\\x00\\x00'))
    255
    >>> read_uint8(io.BytesIO(b'\\xff' * 8)) == 2**64-1
    True
    """

uint8: ArgumentDescriptor

if sys.version_info >= (3, 12):
    def read_stringnl(f: IO[bytes], decode: bool = True, stripquotes: bool = True, *, encoding: str = "latin-1") -> bytes | str:
        """
        >>> import io
        >>> read_stringnl(io.BytesIO(b"'abcd'\\nefg\\n"))
        'abcd'

        >>> read_stringnl(io.BytesIO(b"\\n"))
        Traceback (most recent call last):
        ...
        ValueError: no string quotes around b''

        >>> read_stringnl(io.BytesIO(b"\\n"), stripquotes=False)
        ''

        >>> read_stringnl(io.BytesIO(b"''\\n"))
        ''

        >>> read_stringnl(io.BytesIO(b'"abcd"'))
        Traceback (most recent call last):
        ...
        ValueError: no newline found when trying to read stringnl

        Embedded escapes are undone in the result.
        >>> read_stringnl(io.BytesIO(br"'a\\n\\\\b\\x00c\\td'" + b"\\n'e'"))
        'a\\n\\\\b\\x00c\\td'
        """

else:
    def read_stringnl(f: IO[bytes], decode: bool = True, stripquotes: bool = True) -> bytes | str:
        """
        >>> import io
        >>> read_stringnl(io.BytesIO(b"'abcd'\\nefg\\n"))
        'abcd'

        >>> read_stringnl(io.BytesIO(b"\\n"))
        Traceback (most recent call last):
        ...
        ValueError: no string quotes around b''

        >>> read_stringnl(io.BytesIO(b"\\n"), stripquotes=False)
        ''

        >>> read_stringnl(io.BytesIO(b"''\\n"))
        ''

        >>> read_stringnl(io.BytesIO(b'"abcd"'))
        Traceback (most recent call last):
        ...
        ValueError: no newline found when trying to read stringnl

        Embedded escapes are undone in the result.
        >>> read_stringnl(io.BytesIO(br"'a\\n\\\\b\\x00c\\td'" + b"\\n'e'"))
        'a\\n\\\\b\\x00c\\td'
        """

stringnl: ArgumentDescriptor

def read_stringnl_noescape(f: IO[bytes]) -> str: ...

stringnl_noescape: ArgumentDescriptor

def read_stringnl_noescape_pair(f: IO[bytes]) -> str:
    """
    >>> import io
    >>> read_stringnl_noescape_pair(io.BytesIO(b"Queue\\nEmpty\\njunk"))
    'Queue Empty'
    """

stringnl_noescape_pair: ArgumentDescriptor

def read_string1(f: IO[bytes]) -> str:
    """
    >>> import io
    >>> read_string1(io.BytesIO(b"\\x00"))
    ''
    >>> read_string1(io.BytesIO(b"\\x03abcdef"))
    'abc'
    """

string1: ArgumentDescriptor

def read_string4(f: IO[bytes]) -> str:
    """
    >>> import io
    >>> read_string4(io.BytesIO(b"\\x00\\x00\\x00\\x00abc"))
    ''
    >>> read_string4(io.BytesIO(b"\\x03\\x00\\x00\\x00abcdef"))
    'abc'
    >>> read_string4(io.BytesIO(b"\\x00\\x00\\x00\\x03abcdef"))
    Traceback (most recent call last):
    ...
    ValueError: expected 50331648 bytes in a string4, but only 6 remain
    """

string4: ArgumentDescriptor

def read_bytes1(f: IO[bytes]) -> bytes:
    """
    >>> import io
    >>> read_bytes1(io.BytesIO(b"\\x00"))
    b''
    >>> read_bytes1(io.BytesIO(b"\\x03abcdef"))
    b'abc'
    """

bytes1: ArgumentDescriptor

def read_bytes4(f: IO[bytes]) -> bytes:
    """
    >>> import io
    >>> read_bytes4(io.BytesIO(b"\\x00\\x00\\x00\\x00abc"))
    b''
    >>> read_bytes4(io.BytesIO(b"\\x03\\x00\\x00\\x00abcdef"))
    b'abc'
    >>> read_bytes4(io.BytesIO(b"\\x00\\x00\\x00\\x03abcdef"))
    Traceback (most recent call last):
    ...
    ValueError: expected 50331648 bytes in a bytes4, but only 6 remain
    """

bytes4: ArgumentDescriptor

def read_bytes8(f: IO[bytes]) -> bytes:
    """
    >>> import io, struct, sys
    >>> read_bytes8(io.BytesIO(b"\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00abc"))
    b''
    >>> read_bytes8(io.BytesIO(b"\\x03\\x00\\x00\\x00\\x00\\x00\\x00\\x00abcdef"))
    b'abc'
    >>> bigsize8 = struct.pack("<Q", sys.maxsize//3)
    >>> read_bytes8(io.BytesIO(bigsize8 + b"abcdef"))  #doctest: +ELLIPSIS
    Traceback (most recent call last):
    ...
    ValueError: expected ... bytes in a bytes8, but only 6 remain
    """

bytes8: ArgumentDescriptor

def read_unicodestringnl(f: IO[bytes]) -> str:
    """
    >>> import io
    >>> read_unicodestringnl(io.BytesIO(b"abc\\\\uabcd\\njunk")) == 'abc\\uabcd'
    True
    """

unicodestringnl: ArgumentDescriptor

def read_unicodestring1(f: IO[bytes]) -> str:
    """
    >>> import io
    >>> s = 'abcd\\uabcd'
    >>> enc = s.encode('utf-8')
    >>> enc
    b'abcd\\xea\\xaf\\x8d'
    >>> n = bytes([len(enc)])  # little-endian 1-byte length
    >>> t = read_unicodestring1(io.BytesIO(n + enc + b'junk'))
    >>> s == t
    True

    >>> read_unicodestring1(io.BytesIO(n + enc[:-1]))
    Traceback (most recent call last):
    ...
    ValueError: expected 7 bytes in a unicodestring1, but only 6 remain
    """

unicodestring1: ArgumentDescriptor

def read_unicodestring4(f: IO[bytes]) -> str:
    """
    >>> import io
    >>> s = 'abcd\\uabcd'
    >>> enc = s.encode('utf-8')
    >>> enc
    b'abcd\\xea\\xaf\\x8d'
    >>> n = bytes([len(enc), 0, 0, 0])  # little-endian 4-byte length
    >>> t = read_unicodestring4(io.BytesIO(n + enc + b'junk'))
    >>> s == t
    True

    >>> read_unicodestring4(io.BytesIO(n + enc[:-1]))
    Traceback (most recent call last):
    ...
    ValueError: expected 7 bytes in a unicodestring4, but only 6 remain
    """

unicodestring4: ArgumentDescriptor

def read_unicodestring8(f: IO[bytes]) -> str:
    """
    >>> import io
    >>> s = 'abcd\\uabcd'
    >>> enc = s.encode('utf-8')
    >>> enc
    b'abcd\\xea\\xaf\\x8d'
    >>> n = bytes([len(enc)]) + b'\\0' * 7  # little-endian 8-byte length
    >>> t = read_unicodestring8(io.BytesIO(n + enc + b'junk'))
    >>> s == t
    True

    >>> read_unicodestring8(io.BytesIO(n + enc[:-1]))
    Traceback (most recent call last):
    ...
    ValueError: expected 7 bytes in a unicodestring8, but only 6 remain
    """

unicodestring8: ArgumentDescriptor

def read_decimalnl_short(f: IO[bytes]) -> int:
    """
    >>> import io
    >>> read_decimalnl_short(io.BytesIO(b"1234\\n56"))
    1234

    >>> read_decimalnl_short(io.BytesIO(b"1234L\\n56"))
    Traceback (most recent call last):
    ...
    ValueError: invalid literal for int() with base 10: b'1234L'
    """

def read_decimalnl_long(f: IO[bytes]) -> int:
    """
    >>> import io

    >>> read_decimalnl_long(io.BytesIO(b"1234L\\n56"))
    1234

    >>> read_decimalnl_long(io.BytesIO(b"123456789012345678901234L\\n6"))
    123456789012345678901234
    """

decimalnl_short: ArgumentDescriptor
decimalnl_long: ArgumentDescriptor

def read_floatnl(f: IO[bytes]) -> float:
    """
    >>> import io
    >>> read_floatnl(io.BytesIO(b"-1.25\\n6"))
    -1.25
    """

floatnl: ArgumentDescriptor

def read_float8(f: IO[bytes]) -> float:
    """
    >>> import io, struct
    >>> raw = struct.pack(">d", -1.25)
    >>> raw
    b'\\xbf\\xf4\\x00\\x00\\x00\\x00\\x00\\x00'
    >>> read_float8(io.BytesIO(raw + b"\\n"))
    -1.25
    """

float8: ArgumentDescriptor

def read_long1(f: IO[bytes]) -> int:
    """
    >>> import io
    >>> read_long1(io.BytesIO(b"\\x00"))
    0
    >>> read_long1(io.BytesIO(b"\\x02\\xff\\x00"))
    255
    >>> read_long1(io.BytesIO(b"\\x02\\xff\\x7f"))
    32767
    >>> read_long1(io.BytesIO(b"\\x02\\x00\\xff"))
    -256
    >>> read_long1(io.BytesIO(b"\\x02\\x00\\x80"))
    -32768
    """

long1: ArgumentDescriptor

def read_long4(f: IO[bytes]) -> int:
    """
    >>> import io
    >>> read_long4(io.BytesIO(b"\\x02\\x00\\x00\\x00\\xff\\x00"))
    255
    >>> read_long4(io.BytesIO(b"\\x02\\x00\\x00\\x00\\xff\\x7f"))
    32767
    >>> read_long4(io.BytesIO(b"\\x02\\x00\\x00\\x00\\x00\\xff"))
    -256
    >>> read_long4(io.BytesIO(b"\\x02\\x00\\x00\\x00\\x00\\x80"))
    -32768
    >>> read_long1(io.BytesIO(b"\\x00\\x00\\x00\\x00"))
    0
    """

long4: ArgumentDescriptor

class StackObject:
    __slots__ = ("name", "obtype", "doc")
    name: str
    obtype: type[Any] | tuple[type[Any], ...]
    doc: str
    def __init__(self, name: str, obtype: type[Any] | tuple[type[Any], ...], doc: str) -> None: ...

pyint: StackObject
pylong: StackObject
pyinteger_or_bool: StackObject
pybool: StackObject
pyfloat: StackObject
pybytes_or_str: StackObject
pystring: StackObject
pybytes: StackObject
pyunicode: StackObject
pynone: StackObject
pytuple: StackObject
pylist: StackObject
pydict: StackObject
pyset: StackObject
pyfrozenset: StackObject
anyobject: StackObject
markobject: StackObject
stackslice: StackObject

class OpcodeInfo:
    __slots__ = ("name", "code", "arg", "stack_before", "stack_after", "proto", "doc")
    name: str
    code: str
    arg: ArgumentDescriptor | None
    stack_before: list[StackObject]
    stack_after: list[StackObject]
    proto: int
    doc: str
    def __init__(
        self,
        name: str,
        code: str,
        arg: ArgumentDescriptor | None,
        stack_before: list[StackObject],
        stack_after: list[StackObject],
        proto: int,
        doc: str,
    ) -> None: ...

opcodes: list[OpcodeInfo]

def genops(pickle: bytes | bytearray | IO[bytes]) -> Iterator[tuple[OpcodeInfo, Any | None, int | None]]:
    """Generate all the opcodes in a pickle.

    'pickle' is a file-like object, or string, containing the pickle.

    Each opcode in the pickle is generated, from the current pickle position,
    stopping after a STOP opcode is delivered.  A triple is generated for
    each opcode:

        opcode, arg, pos

    opcode is an OpcodeInfo record, describing the current opcode.

    If the opcode has an argument embedded in the pickle, arg is its decoded
    value, as a Python object.  If the opcode doesn't have an argument, arg
    is None.

    If the pickle has a tell() method, pos was the value of pickle.tell()
    before reading the current opcode.  If the pickle is a bytes object,
    it's wrapped in a BytesIO object, and the latter's tell() result is
    used.  Else (the pickle doesn't have a tell(), and it's not obvious how
    to query its current position) pos is None.
    """

def optimize(p: bytes | bytearray | IO[bytes]) -> bytes:
    """Optimize a pickle string by removing unused PUT opcodes"""

def dis(
    pickle: bytes | bytearray | IO[bytes],
    out: IO[str] | None = None,
    memo: MutableMapping[int, Any] | None = None,
    indentlevel: int = 4,
    annotate: int = 0,
) -> None:
    """Produce a symbolic disassembly of a pickle.

    'pickle' is a file-like object, or string, containing a (at least one)
    pickle.  The pickle is disassembled from the current position, through
    the first STOP opcode encountered.

    Optional arg 'out' is a file-like object to which the disassembly is
    printed.  It defaults to sys.stdout.

    Optional arg 'memo' is a Python dict, used as the pickle's memo.  It
    may be mutated by dis(), if the pickle contains PUT or BINPUT opcodes.
    Passing the same memo object to another dis() call then allows disassembly
    to proceed across multiple pickles that were all created by the same
    pickler with the same memo.  Ordinarily you don't need to worry about this.

    Optional arg 'indentlevel' is the number of blanks by which to indent
    a new MARK level.  It defaults to 4.

    Optional arg 'annotate' if nonzero instructs dis() to add short
    description of the opcode on each line of disassembled output.
    The value given to 'annotate' must be an integer and is used as a
    hint for the column where annotation should start.  The default
    value is 0, meaning no annotations.

    In addition to printing the disassembly, some sanity checks are made:

    + All embedded opcode arguments "make sense".

    + Explicit and implicit pop operations have enough items on the stack.

    + When an opcode implicitly refers to a markobject, a markobject is
      actually on the stack.

    + A memo entry isn't referenced before it's defined.

    + The markobject isn't stored in the memo.
    """
