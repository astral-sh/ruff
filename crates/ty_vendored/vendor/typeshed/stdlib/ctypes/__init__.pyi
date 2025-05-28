import sys
from _ctypes import (
    RTLD_GLOBAL as RTLD_GLOBAL,
    RTLD_LOCAL as RTLD_LOCAL,
    Array as Array,
    CFuncPtr as _CFuncPtr,
    Structure as Structure,
    Union as Union,
    _CanCastTo as _CanCastTo,
    _CArgObject as _CArgObject,
    _CData as _CData,
    _CDataType as _CDataType,
    _CField as _CField,
    _Pointer as _Pointer,
    _PointerLike as _PointerLike,
    _SimpleCData as _SimpleCData,
    addressof as addressof,
    alignment as alignment,
    byref as byref,
    get_errno as get_errno,
    resize as resize,
    set_errno as set_errno,
    sizeof as sizeof,
)
from _typeshed import StrPath
from ctypes._endian import BigEndianStructure as BigEndianStructure, LittleEndianStructure as LittleEndianStructure
from types import GenericAlias
from typing import Any, ClassVar, Generic, Literal, TypeVar, overload, type_check_only
from typing_extensions import Self, TypeAlias, deprecated

if sys.platform == "win32":
    from _ctypes import FormatError as FormatError, get_last_error as get_last_error, set_last_error as set_last_error

if sys.version_info >= (3, 11):
    from ctypes._endian import BigEndianUnion as BigEndianUnion, LittleEndianUnion as LittleEndianUnion

_CT = TypeVar("_CT", bound=_CData)
_T = TypeVar("_T", default=Any)
_DLLT = TypeVar("_DLLT", bound=CDLL)

if sys.version_info >= (3, 14):
    @overload
    @deprecated("ctypes.POINTER with string")
    def POINTER(cls: str) -> type[Any]: ...
    @overload
    def POINTER(cls: None) -> type[c_void_p]: ...
    @overload
    def POINTER(cls: type[_CT]) -> type[_Pointer[_CT]]: ...
    def pointer(obj: _CT) -> _Pointer[_CT]: ...

else:
    from _ctypes import POINTER as POINTER, pointer as pointer

DEFAULT_MODE: int

class ArgumentError(Exception): ...

# defined within CDLL.__init__
# Runtime name is ctypes.CDLL.__init__.<locals>._FuncPtr
@type_check_only
class _CDLLFuncPointer(_CFuncPtr):
    _flags_: ClassVar[int]
    _restype_: ClassVar[type[_CDataType]]

# Not a real class; _CDLLFuncPointer with a __name__ set on it.
@type_check_only
class _NamedFuncPointer(_CDLLFuncPointer):
    __name__: str

if sys.version_info >= (3, 12):
    _NameTypes: TypeAlias = StrPath | None
else:
    _NameTypes: TypeAlias = str | None

class CDLL:
    _func_flags_: ClassVar[int]
    _func_restype_: ClassVar[type[_CDataType]]
    _name: str
    _handle: int
    _FuncPtr: type[_CDLLFuncPointer]
    def __init__(
        self,
        name: _NameTypes,
        mode: int = ...,
        handle: int | None = None,
        use_errno: bool = False,
        use_last_error: bool = False,
        winmode: int | None = None,
    ) -> None: ...
    def __getattr__(self, name: str) -> _NamedFuncPointer: ...
    def __getitem__(self, name_or_ordinal: str) -> _NamedFuncPointer: ...

if sys.platform == "win32":
    class OleDLL(CDLL): ...
    class WinDLL(CDLL): ...

class PyDLL(CDLL): ...

class LibraryLoader(Generic[_DLLT]):
    def __init__(self, dlltype: type[_DLLT]) -> None: ...
    def __getattr__(self, name: str) -> _DLLT: ...
    def __getitem__(self, name: str) -> _DLLT: ...
    def LoadLibrary(self, name: str) -> _DLLT: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

cdll: LibraryLoader[CDLL]
if sys.platform == "win32":
    windll: LibraryLoader[WinDLL]
    oledll: LibraryLoader[OleDLL]
pydll: LibraryLoader[PyDLL]
pythonapi: PyDLL

# Class definition within CFUNCTYPE / WINFUNCTYPE / PYFUNCTYPE
# Names at runtime are
# ctypes.CFUNCTYPE.<locals>.CFunctionType
# ctypes.WINFUNCTYPE.<locals>.WinFunctionType
# ctypes.PYFUNCTYPE.<locals>.CFunctionType
@type_check_only
class _CFunctionType(_CFuncPtr):
    _argtypes_: ClassVar[list[type[_CData | _CDataType]]]
    _restype_: ClassVar[type[_CData | _CDataType] | None]
    _flags_: ClassVar[int]

# Alias for either function pointer type
_FuncPointer: TypeAlias = _CDLLFuncPointer | _CFunctionType  # noqa: Y047  # not used here

def CFUNCTYPE(
    restype: type[_CData | _CDataType] | None,
    *argtypes: type[_CData | _CDataType],
    use_errno: bool = False,
    use_last_error: bool = False,
) -> type[_CFunctionType]: ...

if sys.platform == "win32":
    def WINFUNCTYPE(
        restype: type[_CData | _CDataType] | None,
        *argtypes: type[_CData | _CDataType],
        use_errno: bool = False,
        use_last_error: bool = False,
    ) -> type[_CFunctionType]: ...

def PYFUNCTYPE(restype: type[_CData | _CDataType] | None, *argtypes: type[_CData | _CDataType]) -> type[_CFunctionType]: ...

# Any type that can be implicitly converted to c_void_p when passed as a C function argument.
# (bytes is not included here, see below.)
_CVoidPLike: TypeAlias = _PointerLike | Array[Any] | _CArgObject | int
# Same as above, but including types known to be read-only (i. e. bytes).
# This distinction is not strictly necessary (ctypes doesn't differentiate between const
# and non-const pointers), but it catches errors like memmove(b'foo', buf, 4)
# when memmove(buf, b'foo', 4) was intended.
_CVoidConstPLike: TypeAlias = _CVoidPLike | bytes

_CastT = TypeVar("_CastT", bound=_CanCastTo)

def cast(obj: _CData | _CDataType | _CArgObject | int, typ: type[_CastT]) -> _CastT: ...
def create_string_buffer(init: int | bytes, size: int | None = None) -> Array[c_char]: ...

c_buffer = create_string_buffer

def create_unicode_buffer(init: int | str, size: int | None = None) -> Array[c_wchar]: ...
@deprecated("Deprecated in Python 3.13; removal scheduled for Python 3.15")
def SetPointerType(pointer: type[_Pointer[Any]], cls: Any) -> None: ...
def ARRAY(typ: _CT, len: int) -> Array[_CT]: ...  # Soft Deprecated, no plans to remove

if sys.platform == "win32":
    def DllCanUnloadNow() -> int: ...
    def DllGetClassObject(rclsid: Any, riid: Any, ppv: Any) -> int: ...  # TODO: not documented

    # Actually just an instance of _NamedFuncPointer (aka _CDLLFuncPointer),
    # but we want to set a more specific __call__
    @type_check_only
    class _GetLastErrorFunctionType(_NamedFuncPointer):
        def __call__(self) -> int: ...

    GetLastError: _GetLastErrorFunctionType

# Actually just an instance of _CFunctionType, but we want to set a more
# specific __call__.
@type_check_only
class _MemmoveFunctionType(_CFunctionType):
    def __call__(self, dst: _CVoidPLike, src: _CVoidConstPLike, count: int) -> int: ...

memmove: _MemmoveFunctionType

# Actually just an instance of _CFunctionType, but we want to set a more
# specific __call__.
@type_check_only
class _MemsetFunctionType(_CFunctionType):
    def __call__(self, dst: _CVoidPLike, c: int, count: int) -> int: ...

memset: _MemsetFunctionType

def string_at(ptr: _CVoidConstPLike, size: int = -1) -> bytes: ...

if sys.platform == "win32":
    def WinError(code: int | None = None, descr: str | None = None) -> OSError: ...

def wstring_at(ptr: _CVoidConstPLike, size: int = -1) -> str: ...

class py_object(_CanCastTo, _SimpleCData[_T]):
    _type_: ClassVar[Literal["O"]]

class c_bool(_SimpleCData[bool]):
    _type_: ClassVar[Literal["?"]]
    def __init__(self, value: bool = ...) -> None: ...

class c_byte(_SimpleCData[int]):
    _type_: ClassVar[Literal["b"]]

class c_ubyte(_SimpleCData[int]):
    _type_: ClassVar[Literal["B"]]

class c_short(_SimpleCData[int]):
    _type_: ClassVar[Literal["h"]]

class c_ushort(_SimpleCData[int]):
    _type_: ClassVar[Literal["H"]]

class c_long(_SimpleCData[int]):
    _type_: ClassVar[Literal["l"]]

class c_ulong(_SimpleCData[int]):
    _type_: ClassVar[Literal["L"]]

class c_int(_SimpleCData[int]):  # can be an alias for c_long
    _type_: ClassVar[Literal["i", "l"]]

class c_uint(_SimpleCData[int]):  # can be an alias for c_ulong
    _type_: ClassVar[Literal["I", "L"]]

class c_longlong(_SimpleCData[int]):  # can be an alias for c_long
    _type_: ClassVar[Literal["q", "l"]]

class c_ulonglong(_SimpleCData[int]):  # can be an alias for c_ulong
    _type_: ClassVar[Literal["Q", "L"]]

c_int8 = c_byte
c_uint8 = c_ubyte

class c_int16(_SimpleCData[int]):  # can be an alias for c_short or c_int
    _type_: ClassVar[Literal["h", "i"]]

class c_uint16(_SimpleCData[int]):  # can be an alias for c_ushort or c_uint
    _type_: ClassVar[Literal["H", "I"]]

class c_int32(_SimpleCData[int]):  # can be an alias for c_int or c_long
    _type_: ClassVar[Literal["i", "l"]]

class c_uint32(_SimpleCData[int]):  # can be an alias for c_uint or c_ulong
    _type_: ClassVar[Literal["I", "L"]]

class c_int64(_SimpleCData[int]):  # can be an alias for c_long or c_longlong
    _type_: ClassVar[Literal["l", "q"]]

class c_uint64(_SimpleCData[int]):  # can be an alias for c_ulong or c_ulonglong
    _type_: ClassVar[Literal["L", "Q"]]

class c_ssize_t(_SimpleCData[int]):  # alias for c_int, c_long, or c_longlong
    _type_: ClassVar[Literal["i", "l", "q"]]

class c_size_t(_SimpleCData[int]):  # alias for c_uint, c_ulong, or c_ulonglong
    _type_: ClassVar[Literal["I", "L", "Q"]]

class c_float(_SimpleCData[float]):
    _type_: ClassVar[Literal["f"]]

class c_double(_SimpleCData[float]):
    _type_: ClassVar[Literal["d"]]

class c_longdouble(_SimpleCData[float]):  # can be an alias for c_double
    _type_: ClassVar[Literal["d", "g"]]

if sys.version_info >= (3, 14):
    class c_float_complex(_SimpleCData[complex]):
        _type_: ClassVar[Literal["E"]]

    class c_double_complex(_SimpleCData[complex]):
        _type_: ClassVar[Literal["C"]]

    class c_longdouble_complex(_SimpleCData[complex]):
        _type_: ClassVar[Literal["F"]]

class c_char(_SimpleCData[bytes]):
    _type_: ClassVar[Literal["c"]]
    def __init__(self, value: int | bytes | bytearray = ...) -> None: ...

class c_char_p(_PointerLike, _SimpleCData[bytes | None]):
    _type_: ClassVar[Literal["z"]]
    def __init__(self, value: int | bytes | None = ...) -> None: ...
    @classmethod
    def from_param(cls, value: Any, /) -> Self | _CArgObject: ...

class c_void_p(_PointerLike, _SimpleCData[int | None]):
    _type_: ClassVar[Literal["P"]]
    @classmethod
    def from_param(cls, value: Any, /) -> Self | _CArgObject: ...

c_voidp = c_void_p  # backwards compatibility (to a bug)

class c_wchar(_SimpleCData[str]):
    _type_: ClassVar[Literal["u"]]

class c_wchar_p(_PointerLike, _SimpleCData[str | None]):
    _type_: ClassVar[Literal["Z"]]
    def __init__(self, value: int | str | None = ...) -> None: ...
    @classmethod
    def from_param(cls, value: Any, /) -> Self | _CArgObject: ...

if sys.platform == "win32":
    class HRESULT(_SimpleCData[int]):  # TODO: undocumented
        _type_: ClassVar[Literal["l"]]

if sys.version_info >= (3, 12):
    # At runtime, this is an alias for either c_int32 or c_int64,
    # which are themselves an alias for one of c_int, c_long, or c_longlong
    # This covers all our bases.
    c_time_t: type[c_int32 | c_int64 | c_int | c_long | c_longlong]
