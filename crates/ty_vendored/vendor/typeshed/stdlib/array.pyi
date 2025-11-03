"""This module defines an object type which can efficiently represent
an array of basic values: characters, integers, floating-point
numbers.  Arrays are sequence types and behave very much like lists,
except that the type of objects stored in them is constrained.
"""

import sys
from _typeshed import ReadableBuffer, SupportsRead, SupportsWrite
from collections.abc import Iterable, MutableSequence
from types import GenericAlias
from typing import Any, ClassVar, Literal, SupportsIndex, TypeVar, overload
from typing_extensions import Self, TypeAlias, deprecated, disjoint_base

_IntTypeCode: TypeAlias = Literal["b", "B", "h", "H", "i", "I", "l", "L", "q", "Q"]
_FloatTypeCode: TypeAlias = Literal["f", "d"]
if sys.version_info >= (3, 13):
    _UnicodeTypeCode: TypeAlias = Literal["u", "w"]
else:
    _UnicodeTypeCode: TypeAlias = Literal["u"]
_TypeCode: TypeAlias = _IntTypeCode | _FloatTypeCode | _UnicodeTypeCode

_T = TypeVar("_T", int, float, str)

typecodes: str

@disjoint_base
class array(MutableSequence[_T]):
    """array(typecode [, initializer]) -> array

    Return a new array whose items are restricted by typecode, and
    initialized from the optional initializer value, which must be a list,
    string or iterable over elements of the appropriate type.

    Arrays represent basic values and behave very much like lists, except
    the type of objects stored in them is constrained. The type is specified
    at object creation time by using a type code, which is a single character.
    The following type codes are defined:

        Type code   C Type             Minimum size in bytes
        'b'         signed integer     1
        'B'         unsigned integer   1
        'u'         Unicode character  2 (see note)
        'h'         signed integer     2
        'H'         unsigned integer   2
        'i'         signed integer     2
        'I'         unsigned integer   2
        'l'         signed integer     4
        'L'         unsigned integer   4
        'q'         signed integer     8 (see note)
        'Q'         unsigned integer   8 (see note)
        'f'         floating-point     4
        'd'         floating-point     8

    NOTE: The 'u' typecode corresponds to Python's unicode character. On
    narrow builds this is 2-bytes on wide builds this is 4-bytes.

    NOTE: The 'q' and 'Q' type codes are only available if the platform
    C compiler used to build Python supports 'long long', or, on Windows,
    '__int64'.

    Methods:

    append() -- append a new item to the end of the array
    buffer_info() -- return information giving the current memory info
    byteswap() -- byteswap all the items of the array
    count() -- return number of occurrences of an object
    extend() -- extend array by appending multiple elements from an iterable
    fromfile() -- read items from a file object
    fromlist() -- append items from the list
    frombytes() -- append items from the string
    index() -- return index of first occurrence of an object
    insert() -- insert a new item into the array at a provided position
    pop() -- remove and return item (default last)
    remove() -- remove first occurrence of an object
    reverse() -- reverse the order of the items in the array
    tofile() -- write all items to a file object
    tolist() -- return the array converted to an ordinary list
    tobytes() -- return the array converted to a string

    Attributes:

    typecode -- the typecode character used to create the array
    itemsize -- the length in bytes of one array item
    """

    @property
    def typecode(self) -> _TypeCode:
        """the typecode character used to create the array"""

    @property
    def itemsize(self) -> int:
        """the size, in bytes, of one array item"""

    @overload
    def __new__(
        cls: type[array[int]], typecode: _IntTypeCode, initializer: bytes | bytearray | Iterable[int] = ..., /
    ) -> array[int]: ...
    @overload
    def __new__(
        cls: type[array[float]], typecode: _FloatTypeCode, initializer: bytes | bytearray | Iterable[float] = ..., /
    ) -> array[float]: ...
    if sys.version_info >= (3, 13):
        @overload
        def __new__(
            cls: type[array[str]], typecode: Literal["w"], initializer: bytes | bytearray | Iterable[str] = ..., /
        ) -> array[str]: ...
        @overload
        @deprecated("Deprecated since Python 3.3; will be removed in Python 3.16. Use 'w' typecode instead.")
        def __new__(
            cls: type[array[str]], typecode: Literal["u"], initializer: bytes | bytearray | Iterable[str] = ..., /
        ) -> array[str]: ...
    else:
        @overload
        @deprecated("Deprecated since Python 3.3; will be removed in Python 3.16.")
        def __new__(
            cls: type[array[str]], typecode: Literal["u"], initializer: bytes | bytearray | Iterable[str] = ..., /
        ) -> array[str]: ...

    @overload
    def __new__(cls, typecode: str, initializer: Iterable[_T], /) -> Self: ...
    @overload
    def __new__(cls, typecode: str, initializer: bytes | bytearray = ..., /) -> Self: ...
    def append(self, v: _T, /) -> None:
        """Append new value v to the end of the array."""

    def buffer_info(self) -> tuple[int, int]:
        """Return a tuple (address, length) giving the current memory address and the length in items of the buffer used to hold array's contents.

        The length should be multiplied by the itemsize attribute to calculate
        the buffer length in bytes.
        """

    def byteswap(self) -> None:
        """Byteswap all items of the array.

        If the items in the array are not 1, 2, 4, or 8 bytes in size, RuntimeError is
        raised.
        """

    def count(self, v: _T, /) -> int:
        """Return number of occurrences of v in the array."""

    def extend(self, bb: Iterable[_T], /) -> None:
        """Append items to the end of the array."""

    def frombytes(self, buffer: ReadableBuffer, /) -> None:
        """Appends items from the string, interpreting it as an array of machine values, as if it had been read from a file using the fromfile() method."""

    def fromfile(self, f: SupportsRead[bytes], n: int, /) -> None:
        """Read n objects from the file object f and append them to the end of the array."""

    def fromlist(self, list: list[_T], /) -> None:
        """Append items to array from list."""

    def fromunicode(self, ustr: str, /) -> None:
        """Extends this array with data from the unicode string ustr.

        The array must be a unicode type array; otherwise a ValueError is raised.
        Use array.frombytes(ustr.encode(...)) to append Unicode data to an array of
        some other type.
        """
    if sys.version_info >= (3, 10):
        def index(self, v: _T, start: int = 0, stop: int = sys.maxsize, /) -> int:
            """Return index of first occurrence of v in the array.

            Raise ValueError if the value is not present.
            """
    else:
        def index(self, v: _T, /) -> int:  # type: ignore[override]
            """Return index of first occurrence of v in the array."""

    def insert(self, i: int, v: _T, /) -> None:
        """Insert a new item v into the array before position i."""

    def pop(self, i: int = -1, /) -> _T:
        """Return the i-th element and delete it from the array.

        i defaults to -1.
        """

    def remove(self, v: _T, /) -> None:
        """Remove the first occurrence of v in the array."""

    def tobytes(self) -> bytes:
        """Convert the array to an array of machine values and return the bytes representation."""

    def tofile(self, f: SupportsWrite[bytes], /) -> None:
        """Write all items (as machine values) to the file object f."""

    def tolist(self) -> list[_T]:
        """Convert array to an ordinary list with the same items."""

    def tounicode(self) -> str:
        """Extends this array with data from the unicode string ustr.

        Convert the array to a unicode string.  The array must be a unicode type array;
        otherwise a ValueError is raised.  Use array.tobytes().decode() to obtain a
        unicode string from an array of some other type.
        """
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __contains__(self, value: object, /) -> bool:
        """Return bool(key in self)."""

    def __len__(self) -> int:
        """Return len(self)."""

    @overload
    def __getitem__(self, key: SupportsIndex, /) -> _T:
        """Return self[key]."""

    @overload
    def __getitem__(self, key: slice, /) -> array[_T]: ...
    @overload  # type: ignore[override]
    def __setitem__(self, key: SupportsIndex, value: _T, /) -> None:
        """Set self[key] to value."""

    @overload
    def __setitem__(self, key: slice, value: array[_T], /) -> None: ...
    def __delitem__(self, key: SupportsIndex | slice, /) -> None:
        """Delete self[key]."""

    def __add__(self, value: array[_T], /) -> array[_T]:
        """Return self+value."""

    def __eq__(self, value: object, /) -> bool: ...
    def __ge__(self, value: array[_T], /) -> bool: ...
    def __gt__(self, value: array[_T], /) -> bool: ...
    def __iadd__(self, value: array[_T], /) -> Self:  # type: ignore[override]
        """Implement self+=value."""

    def __imul__(self, value: int, /) -> Self:
        """Implement self*=value."""

    def __le__(self, value: array[_T], /) -> bool: ...
    def __lt__(self, value: array[_T], /) -> bool: ...
    def __mul__(self, value: int, /) -> array[_T]:
        """Return self*value."""

    def __rmul__(self, value: int, /) -> array[_T]:
        """Return value*self."""

    def __copy__(self) -> array[_T]:
        """Return a copy of the array."""

    def __deepcopy__(self, unused: Any, /) -> array[_T]:
        """Return a copy of the array."""

    def __buffer__(self, flags: int, /) -> memoryview:
        """Return a buffer object that exposes the underlying memory of the object."""

    def __release_buffer__(self, buffer: memoryview, /) -> None:
        """Release the buffer object that exposes the underlying memory of the object."""
    if sys.version_info >= (3, 12):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias:
            """See PEP 585"""

ArrayType = array
