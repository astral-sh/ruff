import _typeshed
import sys
from _typeshed import ReadableBuffer, StrOrBytesPath, WriteableBuffer
from abc import abstractmethod
from collections.abc import Callable, Iterable, Iterator, Mapping, Sequence
from ctypes import CDLL, ArgumentError as ArgumentError, c_void_p
from typing import Any, ClassVar, Generic, TypeVar, final, overload, type_check_only
from typing_extensions import Self, TypeAlias

if sys.version_info >= (3, 9):
    from types import GenericAlias

_T = TypeVar("_T")
_CT = TypeVar("_CT", bound=_CData)

FUNCFLAG_CDECL: int
FUNCFLAG_PYTHONAPI: int
FUNCFLAG_USE_ERRNO: int
FUNCFLAG_USE_LASTERROR: int
RTLD_GLOBAL: int
RTLD_LOCAL: int

if sys.version_info >= (3, 11):
    CTYPES_MAX_ARGCOUNT: int

if sys.version_info >= (3, 12):
    SIZEOF_TIME_T: int

if sys.platform == "win32":
    # Description, Source, HelpFile, HelpContext, scode
    _COMError_Details: TypeAlias = tuple[str | None, str | None, str | None, int | None, int | None]

    class COMError(Exception):
        hresult: int
        text: str | None
        details: _COMError_Details

        def __init__(self, hresult: int, text: str | None, details: _COMError_Details) -> None: ...

    def CopyComPointer(src: _PointerLike, dst: _PointerLike | _CArgObject) -> int: ...

    FUNCFLAG_HRESULT: int
    FUNCFLAG_STDCALL: int

    def FormatError(code: int = ...) -> str: ...
    def get_last_error() -> int: ...
    def set_last_error(value: int) -> int: ...
    def LoadLibrary(name: str, load_flags: int = 0, /) -> int: ...
    def FreeLibrary(handle: int, /) -> None: ...

else:
    def dlclose(handle: int, /) -> None: ...
    # The default for flag is RTLD_GLOBAL|RTLD_LOCAL, which is platform dependent.
    def dlopen(name: StrOrBytesPath, flag: int = ..., /) -> int: ...
    def dlsym(handle: int, name: str, /) -> int: ...

if sys.version_info >= (3, 13):
    # This class is not exposed. It calls itself _ctypes.CType_Type.
    @type_check_only
    class _CType_Type(type):
        # By default mypy complains about the following two methods, because strictly speaking cls
        # might not be a Type[_CT]. However this doesn't happen because this is only a
        # metaclass for subclasses of _CData.
        def __mul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]
        def __rmul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]

    _CTypeBaseType = _CType_Type

else:
    _CTypeBaseType = type

# This class is not exposed.
@type_check_only
class _CData:
    _b_base_: int
    _b_needsfree_: bool
    _objects: Mapping[Any, int] | None
    def __buffer__(self, flags: int, /) -> memoryview: ...
    def __ctypes_from_outparam__(self, /) -> Self: ...

# this is a union of all the subclasses of _CData, which is useful because of
# the methods that are present on each of those subclasses which are not present
# on _CData itself.
_CDataType: TypeAlias = _SimpleCData[Any] | _Pointer[Any] | CFuncPtr | Union | Structure | Array[Any]

# This class is not exposed. It calls itself _ctypes.PyCSimpleType.
@type_check_only
class _PyCSimpleType(_CTypeBaseType):
    def from_address(self: type[_typeshed.Self], value: int, /) -> _typeshed.Self: ...
    def from_buffer(self: type[_typeshed.Self], obj: WriteableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_buffer_copy(self: type[_typeshed.Self], buffer: ReadableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_param(self: type[_typeshed.Self], value: Any, /) -> _typeshed.Self | _CArgObject: ...
    def in_dll(self: type[_typeshed.Self], dll: CDLL, name: str, /) -> _typeshed.Self: ...
    if sys.version_info < (3, 13):
        # Inherited from CType_Type starting on 3.13
        def __mul__(self: type[_CT], value: int, /) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]
        def __rmul__(self: type[_CT], value: int, /) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]

class _SimpleCData(_CData, Generic[_T], metaclass=_PyCSimpleType):
    value: _T
    # The TypeVar can be unsolved here,
    # but we can't use overloads without creating many, many mypy false-positive errors
    def __init__(self, value: _T = ...) -> None: ...  # pyright: ignore[reportInvalidTypeVarUse]
    def __ctypes_from_outparam__(self, /) -> _T: ...  # type: ignore[override]

class _CanCastTo(_CData): ...
class _PointerLike(_CanCastTo): ...

# This type is not exposed. It calls itself _ctypes.PyCPointerType.
@type_check_only
class _PyCPointerType(_CTypeBaseType):
    def from_address(self: type[_typeshed.Self], value: int, /) -> _typeshed.Self: ...
    def from_buffer(self: type[_typeshed.Self], obj: WriteableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_buffer_copy(self: type[_typeshed.Self], buffer: ReadableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_param(self: type[_typeshed.Self], value: Any, /) -> _typeshed.Self | _CArgObject: ...
    def in_dll(self: type[_typeshed.Self], dll: CDLL, name: str, /) -> _typeshed.Self: ...
    def set_type(self, type: Any, /) -> None: ...
    if sys.version_info < (3, 13):
        # Inherited from CType_Type starting on 3.13
        def __mul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]
        def __rmul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]

class _Pointer(_PointerLike, _CData, Generic[_CT], metaclass=_PyCPointerType):
    _type_: type[_CT]
    contents: _CT
    @overload
    def __init__(self) -> None: ...
    @overload
    def __init__(self, arg: _CT) -> None: ...
    @overload
    def __getitem__(self, key: int, /) -> Any: ...
    @overload
    def __getitem__(self, key: slice, /) -> list[Any]: ...
    def __setitem__(self, key: int, value: Any, /) -> None: ...

@overload
def POINTER(type: None, /) -> type[c_void_p]: ...
@overload
def POINTER(type: type[_CT], /) -> type[_Pointer[_CT]]: ...
def pointer(obj: _CT, /) -> _Pointer[_CT]: ...

# This class is not exposed. It calls itself _ctypes.CArgObject.
@final
@type_check_only
class _CArgObject: ...

def byref(obj: _CData | _CDataType, offset: int = ...) -> _CArgObject: ...

_ECT: TypeAlias = Callable[[_CData | _CDataType | None, CFuncPtr, tuple[_CData | _CDataType, ...]], _CDataType]
_PF: TypeAlias = tuple[int] | tuple[int, str | None] | tuple[int, str | None, Any]

# This class is not exposed. It calls itself _ctypes.PyCFuncPtrType.
@type_check_only
class _PyCFuncPtrType(_CTypeBaseType):
    def from_address(self: type[_typeshed.Self], value: int, /) -> _typeshed.Self: ...
    def from_buffer(self: type[_typeshed.Self], obj: WriteableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_buffer_copy(self: type[_typeshed.Self], buffer: ReadableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_param(self: type[_typeshed.Self], value: Any, /) -> _typeshed.Self | _CArgObject: ...
    def in_dll(self: type[_typeshed.Self], dll: CDLL, name: str, /) -> _typeshed.Self: ...
    if sys.version_info < (3, 13):
        # Inherited from CType_Type starting on 3.13
        def __mul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]
        def __rmul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]

class CFuncPtr(_PointerLike, _CData, metaclass=_PyCFuncPtrType):
    restype: type[_CDataType] | Callable[[int], Any] | None
    argtypes: Sequence[type[_CDataType]]
    errcheck: _ECT
    # Abstract attribute that must be defined on subclasses
    _flags_: ClassVar[int]
    @overload
    def __new__(cls) -> Self: ...
    @overload
    def __new__(cls, address: int, /) -> Self: ...
    @overload
    def __new__(cls, callable: Callable[..., Any], /) -> Self: ...
    @overload
    def __new__(cls, func_spec: tuple[str | int, CDLL], paramflags: tuple[_PF, ...] | None = ..., /) -> Self: ...
    if sys.platform == "win32":
        @overload
        def __new__(
            cls, vtbl_index: int, name: str, paramflags: tuple[_PF, ...] | None = ..., iid: _CData | _CDataType | None = ..., /
        ) -> Self: ...

    def __call__(self, *args: Any, **kwargs: Any) -> Any: ...

_GetT = TypeVar("_GetT")
_SetT = TypeVar("_SetT")

# This class is not exposed. It calls itself _ctypes.CField.
@final
@type_check_only
class _CField(Generic[_CT, _GetT, _SetT]):
    offset: int
    size: int
    if sys.version_info >= (3, 10):
        @overload
        def __get__(self, instance: None, owner: type[Any] | None = None, /) -> Self: ...
        @overload
        def __get__(self, instance: Any, owner: type[Any] | None = None, /) -> _GetT: ...
    else:
        @overload
        def __get__(self, instance: None, owner: type[Any] | None, /) -> Self: ...
        @overload
        def __get__(self, instance: Any, owner: type[Any] | None, /) -> _GetT: ...

    def __set__(self, instance: Any, value: _SetT, /) -> None: ...

# This class is not exposed. It calls itself _ctypes.UnionType.
@type_check_only
class _UnionType(_CTypeBaseType):
    def from_address(self: type[_typeshed.Self], value: int, /) -> _typeshed.Self: ...
    def from_buffer(self: type[_typeshed.Self], obj: WriteableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_buffer_copy(self: type[_typeshed.Self], buffer: ReadableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_param(self: type[_typeshed.Self], value: Any, /) -> _typeshed.Self | _CArgObject: ...
    def in_dll(self: type[_typeshed.Self], dll: CDLL, name: str, /) -> _typeshed.Self: ...
    # At runtime, various attributes are created on a Union subclass based
    # on its _fields_. This method doesn't exist, but represents those
    # dynamically created attributes.
    def __getattr__(self, name: str) -> _CField[Any, Any, Any]: ...
    if sys.version_info < (3, 13):
        # Inherited from CType_Type starting on 3.13
        def __mul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]
        def __rmul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]

class Union(_CData, metaclass=_UnionType):
    _fields_: ClassVar[Sequence[tuple[str, type[_CDataType]] | tuple[str, type[_CDataType], int]]]
    _pack_: ClassVar[int]
    _anonymous_: ClassVar[Sequence[str]]
    if sys.version_info >= (3, 13):
        _align_: ClassVar[int]

    def __init__(self, *args: Any, **kw: Any) -> None: ...
    def __getattr__(self, name: str) -> Any: ...
    def __setattr__(self, name: str, value: Any) -> None: ...

# This class is not exposed. It calls itself _ctypes.PyCStructType.
@type_check_only
class _PyCStructType(_CTypeBaseType):
    def from_address(self: type[_typeshed.Self], value: int, /) -> _typeshed.Self: ...
    def from_buffer(self: type[_typeshed.Self], obj: WriteableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_buffer_copy(self: type[_typeshed.Self], buffer: ReadableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_param(self: type[_typeshed.Self], value: Any, /) -> _typeshed.Self | _CArgObject: ...
    def in_dll(self: type[_typeshed.Self], dll: CDLL, name: str, /) -> _typeshed.Self: ...
    # At runtime, various attributes are created on a Structure subclass based
    # on its _fields_. This method doesn't exist, but represents those
    # dynamically created attributes.
    def __getattr__(self, name: str) -> _CField[Any, Any, Any]: ...
    if sys.version_info < (3, 13):
        # Inherited from CType_Type starting on 3.13
        def __mul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]
        def __rmul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]

class Structure(_CData, metaclass=_PyCStructType):
    _fields_: ClassVar[Sequence[tuple[str, type[_CDataType]] | tuple[str, type[_CDataType], int]]]
    _pack_: ClassVar[int]
    _anonymous_: ClassVar[Sequence[str]]
    if sys.version_info >= (3, 13):
        _align_: ClassVar[int]

    def __init__(self, *args: Any, **kw: Any) -> None: ...
    def __getattr__(self, name: str) -> Any: ...
    def __setattr__(self, name: str, value: Any) -> None: ...

# This class is not exposed. It calls itself _ctypes.PyCArrayType.
@type_check_only
class _PyCArrayType(_CTypeBaseType):
    def from_address(self: type[_typeshed.Self], value: int, /) -> _typeshed.Self: ...
    def from_buffer(self: type[_typeshed.Self], obj: WriteableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_buffer_copy(self: type[_typeshed.Self], buffer: ReadableBuffer, offset: int = 0, /) -> _typeshed.Self: ...
    def from_param(self: type[_typeshed.Self], value: Any, /) -> _typeshed.Self | _CArgObject: ...
    def in_dll(self: type[_typeshed.Self], dll: CDLL, name: str, /) -> _typeshed.Self: ...
    if sys.version_info < (3, 13):
        # Inherited from CType_Type starting on 3.13
        def __mul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]
        def __rmul__(cls: type[_CT], other: int) -> type[Array[_CT]]: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues]

class Array(_CData, Generic[_CT], metaclass=_PyCArrayType):
    @property
    @abstractmethod
    def _length_(self) -> int: ...
    @_length_.setter
    def _length_(self, value: int) -> None: ...
    @property
    @abstractmethod
    def _type_(self) -> type[_CT]: ...
    @_type_.setter
    def _type_(self, value: type[_CT]) -> None: ...
    # Note: only available if _CT == c_char
    @property
    def raw(self) -> bytes: ...
    @raw.setter
    def raw(self, value: ReadableBuffer) -> None: ...
    value: Any  # Note: bytes if _CT == c_char, str if _CT == c_wchar, unavailable otherwise
    # TODO These methods cannot be annotated correctly at the moment.
    # All of these "Any"s stand for the array's element type, but it's not possible to use _CT
    # here, because of a special feature of ctypes.
    # By default, when accessing an element of an Array[_CT], the returned object has type _CT.
    # However, when _CT is a "simple type" like c_int, ctypes automatically "unboxes" the object
    # and converts it to the corresponding Python primitive. For example, when accessing an element
    # of an Array[c_int], a Python int object is returned, not a c_int.
    # This behavior does *not* apply to subclasses of "simple types".
    # If MyInt is a subclass of c_int, then accessing an element of an Array[MyInt] returns
    # a MyInt, not an int.
    # This special behavior is not easy to model in a stub, so for now all places where
    # the array element type would belong are annotated with Any instead.
    def __init__(self, *args: Any) -> None: ...
    @overload
    def __getitem__(self, key: int, /) -> Any: ...
    @overload
    def __getitem__(self, key: slice, /) -> list[Any]: ...
    @overload
    def __setitem__(self, key: int, value: Any, /) -> None: ...
    @overload
    def __setitem__(self, key: slice, value: Iterable[Any], /) -> None: ...
    def __iter__(self) -> Iterator[Any]: ...
    # Can't inherit from Sized because the metaclass conflict between
    # Sized and _CData prevents using _CDataMeta.
    def __len__(self) -> int: ...
    if sys.version_info >= (3, 9):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

def addressof(obj: _CData | _CDataType, /) -> int: ...
def alignment(obj_or_type: _CData | _CDataType | type[_CData | _CDataType], /) -> int: ...
def get_errno() -> int: ...
def resize(obj: _CData | _CDataType, size: int, /) -> None: ...
def set_errno(value: int, /) -> int: ...
def sizeof(obj_or_type: _CData | _CDataType | type[_CData | _CDataType], /) -> int: ...
def PyObj_FromPtr(address: int, /) -> Any: ...
def Py_DECREF(o: _T, /) -> _T: ...
def Py_INCREF(o: _T, /) -> _T: ...
def buffer_info(o: _CData | _CDataType | type[_CData | _CDataType], /) -> tuple[str, int, tuple[int, ...]]: ...
def call_cdeclfunction(address: int, arguments: tuple[Any, ...], /) -> Any: ...
def call_function(address: int, arguments: tuple[Any, ...], /) -> Any: ...
