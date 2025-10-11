"""This module contains functions that can read and write Python values in
a binary format. The format is specific to Python, but independent of
machine architecture issues.

Not all Python object types are supported; in general, only objects
whose value is independent from a particular invocation of Python can be
written and read by this module. The following types are supported:
None, integers, floating-point numbers, strings, bytes, bytearrays,
tuples, lists, sets, dictionaries, and code objects, where it
should be understood that tuples, lists and dictionaries are only
supported as long as the values contained therein are themselves
supported; and recursive lists and dictionaries should not be written
(they will cause infinite loops).

Variables:

version -- indicates the format that the module uses. Version 0 is the
    historical format, version 1 shares interned strings and version 2
    uses a binary format for floating-point numbers.
    Version 3 shares common object references (New in version 3.4).

Functions:

dump() -- write value to a file
load() -- read value from a file
dumps() -- marshal value as a bytes object
loads() -- read value from a bytes-like object
"""

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
    def dump(value: _Marshallable, file: SupportsWrite[bytes], version: int = 5, /, *, allow_code: bool = True) -> None:
        """Write the value on the open file.

          value
            Must be a supported type.
          file
            Must be a writeable binary file.
          version
            Indicates the data format that dump should use.
          allow_code
            Allow to write code objects.

        If the value has (or contains an object that has) an unsupported type, a
        ValueError exception is raised - but garbage data will also be written
        to the file. The object will not be properly read back by load().
        """

    def dumps(value: _Marshallable, version: int = 5, /, *, allow_code: bool = True) -> bytes:
        """Return the bytes object that would be written to a file by dump(value, file).

          value
            Must be a supported type.
          version
            Indicates the data format that dumps should use.
          allow_code
            Allow to write code objects.

        Raise a ValueError exception if value has (or contains an object that has) an
        unsupported type.
        """

elif sys.version_info >= (3, 13):
    def dump(value: _Marshallable, file: SupportsWrite[bytes], version: int = 4, /, *, allow_code: bool = True) -> None:
        """Write the value on the open file.

          value
            Must be a supported type.
          file
            Must be a writeable binary file.
          version
            Indicates the data format that dump should use.
          allow_code
            Allow to write code objects.

        If the value has (or contains an object that has) an unsupported type, a
        ValueError exception is raised - but garbage data will also be written
        to the file. The object will not be properly read back by load().
        """

    def dumps(value: _Marshallable, version: int = 4, /, *, allow_code: bool = True) -> bytes:
        """Return the bytes object that would be written to a file by dump(value, file).

          value
            Must be a supported type.
          version
            Indicates the data format that dumps should use.
          allow_code
            Allow to write code objects.

        Raise a ValueError exception if value has (or contains an object that has) an
        unsupported type.
        """

else:
    def dump(value: _Marshallable, file: SupportsWrite[bytes], version: int = 4, /) -> None:
        """Write the value on the open file.

          value
            Must be a supported type.
          file
            Must be a writeable binary file.
          version
            Indicates the data format that dump should use.

        If the value has (or contains an object that has) an unsupported type, a
        ValueError exception is raised - but garbage data will also be written
        to the file. The object will not be properly read back by load().
        """

    def dumps(value: _Marshallable, version: int = 4, /) -> bytes:
        """Return the bytes object that would be written to a file by dump(value, file).

          value
            Must be a supported type.
          version
            Indicates the data format that dumps should use.

        Raise a ValueError exception if value has (or contains an object that has) an
        unsupported type.
        """

if sys.version_info >= (3, 13):
    def load(file: SupportsRead[bytes], /, *, allow_code: bool = True) -> Any:
        """Read one value from the open file and return it.

          file
            Must be readable binary file.
          allow_code
            Allow to load code objects.

        If no valid value is read (e.g. because the data has a different Python
        version's incompatible marshal format), raise EOFError, ValueError or
        TypeError.

        Note: If an object containing an unsupported type was marshalled with
        dump(), load() will substitute None for the unmarshallable type.
        """

    def loads(bytes: ReadableBuffer, /, *, allow_code: bool = True) -> Any:
        """Convert the bytes-like object to a value.

          allow_code
            Allow to load code objects.

        If no valid value is found, raise EOFError, ValueError or TypeError.  Extra
        bytes in the input are ignored.
        """

else:
    def load(file: SupportsRead[bytes], /) -> Any:
        """Read one value from the open file and return it.

          file
            Must be readable binary file.

        If no valid value is read (e.g. because the data has a different Python
        version's incompatible marshal format), raise EOFError, ValueError or
        TypeError.

        Note: If an object containing an unsupported type was marshalled with
        dump(), load() will substitute None for the unmarshallable type.
        """

    def loads(bytes: ReadableBuffer, /) -> Any:
        """Convert the bytes-like object to a value.

        If no valid value is found, raise EOFError, ValueError or TypeError.  Extra
        bytes in the input are ignored.
        """
