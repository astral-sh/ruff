"""Built-in functions, types, exceptions, and other objects.

This module provides direct access to all 'built-in'
identifiers of Python; for example, builtins.len is
the full name for the built-in function len().

This module is not normally accessed explicitly by most
applications, but can be useful in modules that provide
objects with the same name as a built-in value, but in
which the built-in of that name is also needed.
"""

import _ast
import _sitebuiltins
import _typeshed
import sys
import types
from _collections_abc import dict_items, dict_keys, dict_values
from _typeshed import (
    AnnotationForm,
    ConvertibleToFloat,
    ConvertibleToInt,
    FileDescriptorOrPath,
    OpenBinaryMode,
    OpenBinaryModeReading,
    OpenBinaryModeUpdating,
    OpenBinaryModeWriting,
    OpenTextMode,
    ReadableBuffer,
    SupportsAdd,
    SupportsAiter,
    SupportsAnext,
    SupportsDivMod,
    SupportsFlush,
    SupportsIter,
    SupportsKeysAndGetItem,
    SupportsLenAndGetItem,
    SupportsNext,
    SupportsRAdd,
    SupportsRDivMod,
    SupportsRichComparison,
    SupportsRichComparisonT,
    SupportsWrite,
)
from collections.abc import Awaitable, Callable, Iterable, Iterator, MutableSet, Reversible, Set as AbstractSet, Sized
from io import BufferedRandom, BufferedReader, BufferedWriter, FileIO, TextIOWrapper
from os import PathLike
from types import CellType, CodeType, GenericAlias, TracebackType

# mypy crashes if any of {ByteString, Sequence, MutableSequence, Mapping, MutableMapping}
# are imported from collections.abc in builtins.pyi
from typing import (  # noqa: Y022,UP035
    IO,
    Any,
    BinaryIO,
    ClassVar,
    Final,
    Generic,
    Mapping,
    MutableMapping,
    MutableSequence,
    Protocol,
    Sequence,
    SupportsAbs,
    SupportsBytes,
    SupportsComplex,
    SupportsFloat,
    SupportsIndex,
    TypeVar,
    final,
    overload,
    type_check_only,
)

# we can't import `Literal` from typing or mypy crashes: see #11247
from typing_extensions import (  # noqa: Y023
    Concatenate,
    Literal,
    LiteralString,
    ParamSpec,
    Self,
    TypeAlias,
    TypeGuard,
    TypeIs,
    TypeVarTuple,
    deprecated,
    disjoint_base,
)

if sys.version_info >= (3, 14):
    from _typeshed import AnnotateFunc

_T = TypeVar("_T")
_I = TypeVar("_I", default=int)
_T_co = TypeVar("_T_co", covariant=True)
_T_contra = TypeVar("_T_contra", contravariant=True)
_R_co = TypeVar("_R_co", covariant=True)
_KT = TypeVar("_KT")
_VT = TypeVar("_VT")
_S = TypeVar("_S")
_T1 = TypeVar("_T1")
_T2 = TypeVar("_T2")
_T3 = TypeVar("_T3")
_T4 = TypeVar("_T4")
_T5 = TypeVar("_T5")
_SupportsNextT_co = TypeVar("_SupportsNextT_co", bound=SupportsNext[Any], covariant=True)
_SupportsAnextT_co = TypeVar("_SupportsAnextT_co", bound=SupportsAnext[Any], covariant=True)
_AwaitableT = TypeVar("_AwaitableT", bound=Awaitable[Any])
_AwaitableT_co = TypeVar("_AwaitableT_co", bound=Awaitable[Any], covariant=True)
_P = ParamSpec("_P")

# Type variables for slice
_StartT_co = TypeVar("_StartT_co", covariant=True, default=Any)  # slice -> slice[Any, Any, Any]
_StopT_co = TypeVar("_StopT_co", covariant=True, default=_StartT_co)  #  slice[A] -> slice[A, A, A]
# NOTE: step could differ from start and stop, (e.g. datetime/timedelta)l
#   the default (start|stop) is chosen to cater to the most common case of int/index slices.
# FIXME: https://github.com/python/typing/issues/213 (replace step=start|stop with step=start&stop)
_StepT_co = TypeVar("_StepT_co", covariant=True, default=_StartT_co | _StopT_co)  #  slice[A,B] -> slice[A, B, A|B]

@disjoint_base
class object:
    """The base class of the class hierarchy.

    When called, it accepts no arguments and returns a new featureless
    instance that has no instance attributes and cannot be given any.
    """

    __doc__: str | None
    __dict__: dict[str, Any]
    __module__: str
    __annotations__: dict[str, Any]
    @property
    def __class__(self) -> type[Self]: ...
    @__class__.setter
    def __class__(self, type: type[Self], /) -> None: ...
    def __init__(self) -> None: ...
    def __new__(cls) -> Self: ...
    # N.B. `object.__setattr__` and `object.__delattr__` are heavily special-cased by type checkers.
    # Overriding them in subclasses has different semantics, even if the override has an identical signature.
    def __setattr__(self, name: str, value: Any, /) -> None: ...
    def __delattr__(self, name: str, /) -> None: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __ne__(self, value: object, /) -> bool: ...
    def __str__(self) -> str: ...  # noqa: Y029
    def __repr__(self) -> str: ...  # noqa: Y029
    def __hash__(self) -> int: ...
    def __format__(self, format_spec: str, /) -> str: ...
    def __getattribute__(self, name: str, /) -> Any: ...
    def __sizeof__(self) -> int: ...
    # return type of pickle methods is rather hard to express in the current type system
    # see #6661 and https://docs.python.org/3/library/pickle.html#object.__reduce__
    def __reduce__(self) -> str | tuple[Any, ...]: ...
    def __reduce_ex__(self, protocol: SupportsIndex, /) -> str | tuple[Any, ...]: ...
    if sys.version_info >= (3, 11):
        def __getstate__(self) -> object: ...

    def __dir__(self) -> Iterable[str]: ...
    def __init_subclass__(cls) -> None: ...
    @classmethod
    def __subclasshook__(cls, subclass: type, /) -> bool: ...

@disjoint_base
class staticmethod(Generic[_P, _R_co]):
    """Convert a function to be a static method.

    A static method does not receive an implicit first argument.
    To declare a static method, use this idiom:

         class C:
             @staticmethod
             def f(arg1, arg2, argN):
                 ...

    It can be called either on the class (e.g. C.f()) or on an instance
    (e.g. C().f()). Both the class and the instance are ignored, and
    neither is passed implicitly as the first argument to the method.

    Static methods in Python are similar to those found in Java or C++.
    For a more advanced concept, see the classmethod builtin.
    """

    @property
    def __func__(self) -> Callable[_P, _R_co]: ...
    @property
    def __isabstractmethod__(self) -> bool: ...
    def __init__(self, f: Callable[_P, _R_co], /) -> None: ...
    @overload
    def __get__(self, instance: None, owner: type, /) -> Callable[_P, _R_co]:
        """Return an attribute of instance, which is of type owner."""

    @overload
    def __get__(self, instance: _T, owner: type[_T] | None = None, /) -> Callable[_P, _R_co]: ...
    if sys.version_info >= (3, 10):
        __name__: str
        __qualname__: str
        @property
        def __wrapped__(self) -> Callable[_P, _R_co]: ...
        def __call__(self, *args: _P.args, **kwargs: _P.kwargs) -> _R_co:
            """Call self as a function."""
    if sys.version_info >= (3, 14):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...
        __annotate__: AnnotateFunc | None

@disjoint_base
class classmethod(Generic[_T, _P, _R_co]):
    """Convert a function to be a class method.

    A class method receives the class as implicit first argument,
    just like an instance method receives the instance.
    To declare a class method, use this idiom:

      class C:
          @classmethod
          def f(cls, arg1, arg2, argN):
              ...

    It can be called either on the class (e.g. C.f()) or on an instance
    (e.g. C().f()).  The instance is ignored except for its class.
    If a class method is called for a derived class, the derived class
    object is passed as the implied first argument.

    Class methods are different than C++ or Java static methods.
    If you want those, see the staticmethod builtin.
    """

    @property
    def __func__(self) -> Callable[Concatenate[type[_T], _P], _R_co]: ...
    @property
    def __isabstractmethod__(self) -> bool: ...
    def __init__(self, f: Callable[Concatenate[type[_T], _P], _R_co], /) -> None: ...
    @overload
    def __get__(self, instance: _T, owner: type[_T] | None = None, /) -> Callable[_P, _R_co]:
        """Return an attribute of instance, which is of type owner."""

    @overload
    def __get__(self, instance: None, owner: type[_T], /) -> Callable[_P, _R_co]: ...
    if sys.version_info >= (3, 10):
        __name__: str
        __qualname__: str
        @property
        def __wrapped__(self) -> Callable[Concatenate[type[_T], _P], _R_co]: ...
    if sys.version_info >= (3, 14):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...
        __annotate__: AnnotateFunc | None

@disjoint_base
class type:
    """type(object) -> the object's type
    type(name, bases, dict, **kwds) -> a new type
    """

    # object.__base__ is None. Otherwise, it would be a type.
    @property
    def __base__(self) -> type | None: ...
    __bases__: tuple[type, ...]
    @property
    def __basicsize__(self) -> int: ...
    # type.__dict__ is read-only at runtime, but that can't be expressed currently.
    # See https://github.com/python/typeshed/issues/11033 for a discussion.
    __dict__: Final[types.MappingProxyType[str, Any]]  # type: ignore[assignment]
    @property
    def __dictoffset__(self) -> int: ...
    @property
    def __flags__(self) -> int: ...
    @property
    def __itemsize__(self) -> int: ...
    __module__: str
    @property
    def __mro__(self) -> tuple[type, ...]: ...
    __name__: str
    __qualname__: str
    @property
    def __text_signature__(self) -> str | None: ...
    @property
    def __weakrefoffset__(self) -> int: ...
    @overload
    def __init__(self, o: object, /) -> None: ...
    @overload
    def __init__(self, name: str, bases: tuple[type, ...], dict: dict[str, Any], /, **kwds: Any) -> None: ...
    @overload
    def __new__(cls, o: object, /) -> type: ...
    @overload
    def __new__(
        cls: type[_typeshed.Self], name: str, bases: tuple[type, ...], namespace: dict[str, Any], /, **kwds: Any
    ) -> _typeshed.Self: ...
    def __call__(self, *args: Any, **kwds: Any) -> Any:
        """Call self as a function."""

    def __subclasses__(self: _typeshed.Self) -> list[_typeshed.Self]:
        """Return a list of immediate subclasses."""
    # Note: the documentation doesn't specify what the return type is, the standard
    # implementation seems to be returning a list.
    def mro(self) -> list[type]:
        """Return a type's method resolution order."""

    def __instancecheck__(self, instance: Any, /) -> bool:
        """Check if an object is an instance."""

    def __subclasscheck__(self, subclass: type, /) -> bool:
        """Check if a class is a subclass."""

    @classmethod
    def __prepare__(metacls, name: str, bases: tuple[type, ...], /, **kwds: Any) -> MutableMapping[str, object]:
        """Create the namespace for the class statement"""
    if sys.version_info >= (3, 10):
        # `int | str` produces an instance of `UnionType`, but `int | int` produces an instance of `type`,
        # and `abc.ABC | abc.ABC` produces an instance of `abc.ABCMeta`.
        def __or__(self: _typeshed.Self, value: Any, /) -> types.UnionType | _typeshed.Self:
            """Return self|value."""

        def __ror__(self: _typeshed.Self, value: Any, /) -> types.UnionType | _typeshed.Self:
            """Return value|self."""
    if sys.version_info >= (3, 12):
        __type_params__: tuple[TypeVar | ParamSpec | TypeVarTuple, ...]
    __annotations__: dict[str, AnnotationForm]
    if sys.version_info >= (3, 14):
        __annotate__: AnnotateFunc | None

@disjoint_base
class super:
    """super() -> same as super(__class__, <first argument>)
    super(type) -> unbound super object
    super(type, obj) -> bound super object; requires isinstance(obj, type)
    super(type, type2) -> bound super object; requires issubclass(type2, type)
    Typical use to call a cooperative superclass method:
    class C(B):
        def meth(self, arg):
            super().meth(arg)
    This works for class methods too:
    class C(B):
        @classmethod
        def cmeth(cls, arg):
            super().cmeth(arg)
    """

    @overload
    def __init__(self, t: Any, obj: Any, /) -> None: ...
    @overload
    def __init__(self, t: Any, /) -> None: ...
    @overload
    def __init__(self) -> None: ...

_PositiveInteger: TypeAlias = Literal[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]
_NegativeInteger: TypeAlias = Literal[-1, -2, -3, -4, -5, -6, -7, -8, -9, -10, -11, -12, -13, -14, -15, -16, -17, -18, -19, -20]
_LiteralInteger = _PositiveInteger | _NegativeInteger | Literal[0]  # noqa: Y026  # TODO: Use TypeAlias once mypy bugs are fixed

@disjoint_base
class int:
    """int([x]) -> integer
    int(x, base=10) -> integer

    Convert a number or string to an integer, or return 0 if no arguments
    are given.  If x is a number, return x.__int__().  For floating-point
    numbers, this truncates towards zero.

    If x is not a number or if base is given, then x must be a string,
    bytes, or bytearray instance representing an integer literal in the
    given base.  The literal can be preceded by '+' or '-' and be surrounded
    by whitespace.  The base defaults to 10.  Valid bases are 0 and 2-36.
    Base 0 means to interpret the base from the string as an integer literal.
    >>> int('0b100', base=0)
    4
    """

    @overload
    def __new__(cls, x: ConvertibleToInt = 0, /) -> Self: ...
    @overload
    def __new__(cls, x: str | bytes | bytearray, /, base: SupportsIndex) -> Self: ...
    def as_integer_ratio(self) -> tuple[int, Literal[1]]:
        """Return a pair of integers, whose ratio is equal to the original int.

        The ratio is in lowest terms and has a positive denominator.

        >>> (10).as_integer_ratio()
        (10, 1)
        >>> (-10).as_integer_ratio()
        (-10, 1)
        >>> (0).as_integer_ratio()
        (0, 1)
        """

    @property
    def real(self) -> int:
        """the real part of a complex number"""

    @property
    def imag(self) -> Literal[0]:
        """the imaginary part of a complex number"""

    @property
    def numerator(self) -> int:
        """the numerator of a rational number in lowest terms"""

    @property
    def denominator(self) -> Literal[1]:
        """the denominator of a rational number in lowest terms"""

    def conjugate(self) -> int:
        """Returns self, the complex conjugate of any int."""

    def bit_length(self) -> int:
        """Number of bits necessary to represent self in binary.

        >>> bin(37)
        '0b100101'
        >>> (37).bit_length()
        6
        """
    if sys.version_info >= (3, 10):
        def bit_count(self) -> int:
            """Number of ones in the binary representation of the absolute value of self.

            Also known as the population count.

            >>> bin(13)
            '0b1101'
            >>> (13).bit_count()
            3
            """
    if sys.version_info >= (3, 11):
        def to_bytes(
            self, length: SupportsIndex = 1, byteorder: Literal["little", "big"] = "big", *, signed: bool = False
        ) -> bytes:
            """Return an array of bytes representing an integer.

            length
              Length of bytes object to use.  An OverflowError is raised if the
              integer is not representable with the given number of bytes.  Default
              is length 1.
            byteorder
              The byte order used to represent the integer.  If byteorder is 'big',
              the most significant byte is at the beginning of the byte array.  If
              byteorder is 'little', the most significant byte is at the end of the
              byte array.  To request the native byte order of the host system, use
              sys.byteorder as the byte order value.  Default is to use 'big'.
            signed
              Determines whether two's complement is used to represent the integer.
              If signed is False and a negative integer is given, an OverflowError
              is raised.
            """

        @classmethod
        def from_bytes(
            cls,
            bytes: Iterable[SupportsIndex] | SupportsBytes | ReadableBuffer,
            byteorder: Literal["little", "big"] = "big",
            *,
            signed: bool = False,
        ) -> Self:
            """Return the integer represented by the given array of bytes.

            bytes
              Holds the array of bytes to convert.  The argument must either
              support the buffer protocol or be an iterable object producing bytes.
              Bytes and bytearray are examples of built-in objects that support the
              buffer protocol.
            byteorder
              The byte order used to represent the integer.  If byteorder is 'big',
              the most significant byte is at the beginning of the byte array.  If
              byteorder is 'little', the most significant byte is at the end of the
              byte array.  To request the native byte order of the host system, use
              sys.byteorder as the byte order value.  Default is to use 'big'.
            signed
              Indicates whether two's complement is used to represent the integer.
            """
    else:
        def to_bytes(self, length: SupportsIndex, byteorder: Literal["little", "big"], *, signed: bool = False) -> bytes:
            """Return an array of bytes representing an integer.

            length
              Length of bytes object to use.  An OverflowError is raised if the
              integer is not representable with the given number of bytes.
            byteorder
              The byte order used to represent the integer.  If byteorder is 'big',
              the most significant byte is at the beginning of the byte array.  If
              byteorder is 'little', the most significant byte is at the end of the
              byte array.  To request the native byte order of the host system, use
              `sys.byteorder' as the byte order value.
            signed
              Determines whether two's complement is used to represent the integer.
              If signed is False and a negative integer is given, an OverflowError
              is raised.
            """

        @classmethod
        def from_bytes(
            cls,
            bytes: Iterable[SupportsIndex] | SupportsBytes | ReadableBuffer,
            byteorder: Literal["little", "big"],
            *,
            signed: bool = False,
        ) -> Self:
            """Return the integer represented by the given array of bytes.

            bytes
              Holds the array of bytes to convert.  The argument must either
              support the buffer protocol or be an iterable object producing bytes.
              Bytes and bytearray are examples of built-in objects that support the
              buffer protocol.
            byteorder
              The byte order used to represent the integer.  If byteorder is 'big',
              the most significant byte is at the beginning of the byte array.  If
              byteorder is 'little', the most significant byte is at the end of the
              byte array.  To request the native byte order of the host system, use
              `sys.byteorder' as the byte order value.
            signed
              Indicates whether two's complement is used to represent the integer.
            """
    if sys.version_info >= (3, 12):
        def is_integer(self) -> Literal[True]:
            """Returns True. Exists for duck type compatibility with float.is_integer."""

    def __add__(self, value: int, /) -> int:
        """Return self+value."""

    def __sub__(self, value: int, /) -> int:
        """Return self-value."""

    def __mul__(self, value: int, /) -> int:
        """Return self*value."""

    def __floordiv__(self, value: int, /) -> int:
        """Return self//value."""

    def __truediv__(self, value: int, /) -> float:
        """Return self/value."""

    def __mod__(self, value: int, /) -> int:
        """Return self%value."""

    def __divmod__(self, value: int, /) -> tuple[int, int]:
        """Return divmod(self, value)."""

    def __radd__(self, value: int, /) -> int:
        """Return value+self."""

    def __rsub__(self, value: int, /) -> int:
        """Return value-self."""

    def __rmul__(self, value: int, /) -> int:
        """Return value*self."""

    def __rfloordiv__(self, value: int, /) -> int:
        """Return value//self."""

    def __rtruediv__(self, value: int, /) -> float:
        """Return value/self."""

    def __rmod__(self, value: int, /) -> int:
        """Return value%self."""

    def __rdivmod__(self, value: int, /) -> tuple[int, int]:
        """Return divmod(value, self)."""

    @overload
    def __pow__(self, x: Literal[0], /) -> Literal[1]:
        """Return pow(self, value, mod)."""

    @overload
    def __pow__(self, value: Literal[0], mod: None, /) -> Literal[1]: ...
    @overload
    def __pow__(self, value: _PositiveInteger, mod: None = None, /) -> int: ...
    @overload
    def __pow__(self, value: _NegativeInteger, mod: None = None, /) -> float: ...
    # positive __value -> int; negative __value -> float
    # return type must be Any as `int | float` causes too many false-positive errors
    @overload
    def __pow__(self, value: int, mod: None = None, /) -> Any: ...
    @overload
    def __pow__(self, value: int, mod: int, /) -> int: ...
    def __rpow__(self, value: int, mod: int | None = None, /) -> Any:
        """Return pow(value, self, mod)."""

    def __and__(self, value: int, /) -> int:
        """Return self&value."""

    def __or__(self, value: int, /) -> int:
        """Return self|value."""

    def __xor__(self, value: int, /) -> int:
        """Return self^value."""

    def __lshift__(self, value: int, /) -> int:
        """Return self<<value."""

    def __rshift__(self, value: int, /) -> int:
        """Return self>>value."""

    def __rand__(self, value: int, /) -> int:
        """Return value&self."""

    def __ror__(self, value: int, /) -> int:
        """Return value|self."""

    def __rxor__(self, value: int, /) -> int:
        """Return value^self."""

    def __rlshift__(self, value: int, /) -> int:
        """Return value<<self."""

    def __rrshift__(self, value: int, /) -> int:
        """Return value>>self."""

    def __neg__(self) -> int:
        """-self"""

    def __pos__(self) -> int:
        """+self"""

    def __invert__(self) -> int:
        """~self"""

    def __trunc__(self) -> int:
        """Truncating an Integral returns itself."""

    def __ceil__(self) -> int:
        """Ceiling of an Integral returns itself."""

    def __floor__(self) -> int:
        """Flooring an Integral returns itself."""
    if sys.version_info >= (3, 14):
        def __round__(self, ndigits: SupportsIndex | None = None, /) -> int:
            """Rounding an Integral returns itself.

            Rounding with an ndigits argument also returns an integer.
            """
    else:
        def __round__(self, ndigits: SupportsIndex = ..., /) -> int:
            """Rounding an Integral returns itself.

            Rounding with an ndigits argument also returns an integer.
            """

    def __getnewargs__(self) -> tuple[int]: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __ne__(self, value: object, /) -> bool: ...
    def __lt__(self, value: int, /) -> bool: ...
    def __le__(self, value: int, /) -> bool: ...
    def __gt__(self, value: int, /) -> bool: ...
    def __ge__(self, value: int, /) -> bool: ...
    def __float__(self) -> float:
        """float(self)"""

    def __int__(self) -> int:
        """int(self)"""

    def __abs__(self) -> int:
        """abs(self)"""

    def __hash__(self) -> int: ...
    def __bool__(self) -> bool:
        """True if self else False"""

    def __index__(self) -> int:
        """Return self converted to an integer, if self is suitable for use as an index into a list."""

    def __format__(self, format_spec: str, /) -> str:
        """Convert to a string according to format_spec."""

@disjoint_base
class float:
    """Convert a string or number to a floating-point number, if possible."""

    def __new__(cls, x: ConvertibleToFloat = 0, /) -> Self: ...
    def as_integer_ratio(self) -> tuple[int, int]:
        """Return a pair of integers, whose ratio is exactly equal to the original float.

        The ratio is in lowest terms and has a positive denominator.  Raise
        OverflowError on infinities and a ValueError on NaNs.

        >>> (10.0).as_integer_ratio()
        (10, 1)
        >>> (0.0).as_integer_ratio()
        (0, 1)
        >>> (-.25).as_integer_ratio()
        (-1, 4)
        """

    def hex(self) -> str:
        """Return a hexadecimal representation of a floating-point number.

        >>> (-0.1).hex()
        '-0x1.999999999999ap-4'
        >>> 3.14159.hex()
        '0x1.921f9f01b866ep+1'
        """

    def is_integer(self) -> bool:
        """Return True if the float is an integer."""

    @classmethod
    def fromhex(cls, string: str, /) -> Self:
        """Create a floating-point number from a hexadecimal string.

        >>> float.fromhex('0x1.ffffp10')
        2047.984375
        >>> float.fromhex('-0x1p-1074')
        -5e-324
        """

    @property
    def real(self) -> float:
        """the real part of a complex number"""

    @property
    def imag(self) -> float:
        """the imaginary part of a complex number"""

    def conjugate(self) -> float:
        """Return self, the complex conjugate of any float."""

    def __add__(self, value: float, /) -> float:
        """Return self+value."""

    def __sub__(self, value: float, /) -> float:
        """Return self-value."""

    def __mul__(self, value: float, /) -> float:
        """Return self*value."""

    def __floordiv__(self, value: float, /) -> float:
        """Return self//value."""

    def __truediv__(self, value: float, /) -> float:
        """Return self/value."""

    def __mod__(self, value: float, /) -> float:
        """Return self%value."""

    def __divmod__(self, value: float, /) -> tuple[float, float]:
        """Return divmod(self, value)."""

    @overload
    def __pow__(self, value: int, mod: None = None, /) -> float:
        """Return pow(self, value, mod)."""
    # positive __value -> float; negative __value -> complex
    # return type must be Any as `float | complex` causes too many false-positive errors
    @overload
    def __pow__(self, value: float, mod: None = None, /) -> Any: ...
    def __radd__(self, value: float, /) -> float:
        """Return value+self."""

    def __rsub__(self, value: float, /) -> float:
        """Return value-self."""

    def __rmul__(self, value: float, /) -> float:
        """Return value*self."""

    def __rfloordiv__(self, value: float, /) -> float:
        """Return value//self."""

    def __rtruediv__(self, value: float, /) -> float:
        """Return value/self."""

    def __rmod__(self, value: float, /) -> float:
        """Return value%self."""

    def __rdivmod__(self, value: float, /) -> tuple[float, float]:
        """Return divmod(value, self)."""

    @overload
    def __rpow__(self, value: _PositiveInteger, mod: None = None, /) -> float:
        """Return pow(value, self, mod)."""

    @overload
    def __rpow__(self, value: _NegativeInteger, mod: None = None, /) -> complex: ...
    # Returning `complex` for the general case gives too many false-positive errors.
    @overload
    def __rpow__(self, value: float, mod: None = None, /) -> Any: ...
    def __getnewargs__(self) -> tuple[float]: ...
    def __trunc__(self) -> int:
        """Return the Integral closest to x between 0 and x."""

    def __ceil__(self) -> int:
        """Return the ceiling as an Integral."""

    def __floor__(self) -> int:
        """Return the floor as an Integral."""

    @overload
    def __round__(self, ndigits: None = None, /) -> int:
        """Return the Integral closest to x, rounding half toward even.

        When an argument is passed, work like built-in round(x, ndigits).
        """

    @overload
    def __round__(self, ndigits: SupportsIndex, /) -> float: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __ne__(self, value: object, /) -> bool: ...
    def __lt__(self, value: float, /) -> bool: ...
    def __le__(self, value: float, /) -> bool: ...
    def __gt__(self, value: float, /) -> bool: ...
    def __ge__(self, value: float, /) -> bool: ...
    def __neg__(self) -> float:
        """-self"""

    def __pos__(self) -> float:
        """+self"""

    def __int__(self) -> int:
        """int(self)"""

    def __float__(self) -> float:
        """float(self)"""

    def __abs__(self) -> float:
        """abs(self)"""

    def __hash__(self) -> int: ...
    def __bool__(self) -> bool:
        """True if self else False"""

    def __format__(self, format_spec: str, /) -> str:
        """Formats the float according to format_spec."""
    if sys.version_info >= (3, 14):
        @classmethod
        def from_number(cls, number: float | SupportsIndex | SupportsFloat, /) -> Self:
            """Convert real number to a floating-point number."""

@disjoint_base
class complex:
    """Create a complex number from a string or numbers.

    If a string is given, parse it as a complex number.
    If a single number is given, convert it to a complex number.
    If the 'real' or 'imag' arguments are given, create a complex number
    with the specified real and imaginary components.
    """

    # Python doesn't currently accept SupportsComplex for the second argument
    @overload
    def __new__(
        cls,
        real: complex | SupportsComplex | SupportsFloat | SupportsIndex = 0,
        imag: complex | SupportsFloat | SupportsIndex = 0,
    ) -> Self: ...
    @overload
    def __new__(cls, real: str | SupportsComplex | SupportsFloat | SupportsIndex | complex) -> Self: ...
    @property
    def real(self) -> float:
        """the real part of a complex number"""

    @property
    def imag(self) -> float:
        """the imaginary part of a complex number"""

    def conjugate(self) -> complex:
        """Return the complex conjugate of its argument. (3-4j).conjugate() == 3+4j."""

    def __add__(self, value: complex, /) -> complex:
        """Return self+value."""

    def __sub__(self, value: complex, /) -> complex:
        """Return self-value."""

    def __mul__(self, value: complex, /) -> complex:
        """Return self*value."""

    def __pow__(self, value: complex, mod: None = None, /) -> complex:
        """Return pow(self, value, mod)."""

    def __truediv__(self, value: complex, /) -> complex:
        """Return self/value."""

    def __radd__(self, value: complex, /) -> complex:
        """Return value+self."""

    def __rsub__(self, value: complex, /) -> complex:
        """Return value-self."""

    def __rmul__(self, value: complex, /) -> complex:
        """Return value*self."""

    def __rpow__(self, value: complex, mod: None = None, /) -> complex:
        """Return pow(value, self, mod)."""

    def __rtruediv__(self, value: complex, /) -> complex:
        """Return value/self."""

    def __eq__(self, value: object, /) -> bool: ...
    def __ne__(self, value: object, /) -> bool: ...
    def __neg__(self) -> complex:
        """-self"""

    def __pos__(self) -> complex:
        """+self"""

    def __abs__(self) -> float:
        """abs(self)"""

    def __hash__(self) -> int: ...
    def __bool__(self) -> bool:
        """True if self else False"""

    def __format__(self, format_spec: str, /) -> str:
        """Convert to a string according to format_spec."""
    if sys.version_info >= (3, 11):
        def __complex__(self) -> complex:
            """Convert this value to exact type complex."""
    if sys.version_info >= (3, 14):
        @classmethod
        def from_number(cls, number: complex | SupportsComplex | SupportsFloat | SupportsIndex, /) -> Self:
            """Convert number to a complex floating-point number."""

@type_check_only
class _FormatMapMapping(Protocol):
    def __getitem__(self, key: str, /) -> Any: ...

@type_check_only
class _TranslateTable(Protocol):
    def __getitem__(self, key: int, /) -> str | int | None: ...

@disjoint_base
class str(Sequence[str]):
    """str(object='') -> str
    str(bytes_or_buffer[, encoding[, errors]]) -> str

    Create a new string object from the given object. If encoding or
    errors is specified, then the object must expose a data buffer
    that will be decoded using the given encoding and error handler.
    Otherwise, returns the result of object.__str__() (if defined)
    or repr(object).
    encoding defaults to 'utf-8'.
    errors defaults to 'strict'.
    """

    @overload
    def __new__(cls, object: object = "") -> Self: ...
    @overload
    def __new__(cls, object: ReadableBuffer, encoding: str = "utf-8", errors: str = "strict") -> Self: ...
    @overload
    def capitalize(self: LiteralString) -> LiteralString:
        """Return a capitalized version of the string.

        More specifically, make the first character have upper case and the rest lower
        case.
        """

    @overload
    def capitalize(self) -> str: ...  # type: ignore[misc]
    @overload
    def casefold(self: LiteralString) -> LiteralString:
        """Return a version of the string suitable for caseless comparisons."""

    @overload
    def casefold(self) -> str: ...  # type: ignore[misc]
    @overload
    def center(self: LiteralString, width: SupportsIndex, fillchar: LiteralString = " ", /) -> LiteralString:
        """Return a centered string of length width.

        Padding is done using the specified fill character (default is a space).
        """

    @overload
    def center(self, width: SupportsIndex, fillchar: str = " ", /) -> str: ...  # type: ignore[misc]
    def count(self, sub: str, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /) -> int:
        """Return the number of non-overlapping occurrences of substring sub in string S[start:end].

        Optional arguments start and end are interpreted as in slice notation.
        """

    def encode(self, encoding: str = "utf-8", errors: str = "strict") -> bytes:
        """Encode the string using the codec registered for encoding.

        encoding
          The encoding in which to encode the string.
        errors
          The error handling scheme to use for encoding errors.
          The default is 'strict' meaning that encoding errors raise a
          UnicodeEncodeError.  Other possible values are 'ignore', 'replace' and
          'xmlcharrefreplace' as well as any other name registered with
          codecs.register_error that can handle UnicodeEncodeErrors.
        """

    def endswith(
        self, suffix: str | tuple[str, ...], start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> bool:
        """Return True if the string ends with the specified suffix, False otherwise.

        suffix
          A string or a tuple of strings to try.
        start
          Optional start position. Default: start of the string.
        end
          Optional stop position. Default: end of the string.
        """

    @overload
    def expandtabs(self: LiteralString, tabsize: SupportsIndex = 8) -> LiteralString:
        """Return a copy where all tab characters are expanded using spaces.

        If tabsize is not given, a tab size of 8 characters is assumed.
        """

    @overload
    def expandtabs(self, tabsize: SupportsIndex = 8) -> str: ...  # type: ignore[misc]
    def find(self, sub: str, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /) -> int:
        """Return the lowest index in S where substring sub is found, such that sub is contained within S[start:end].

        Optional arguments start and end are interpreted as in slice notation.
        Return -1 on failure.
        """

    @overload
    def format(self: LiteralString, *args: LiteralString, **kwargs: LiteralString) -> LiteralString:
        """Return a formatted version of the string, using substitutions from args and kwargs.
        The substitutions are identified by braces ('{' and '}').
        """

    @overload
    def format(self, *args: object, **kwargs: object) -> str: ...
    def format_map(self, mapping: _FormatMapMapping, /) -> str:
        """Return a formatted version of the string, using substitutions from mapping.
        The substitutions are identified by braces ('{' and '}').
        """

    def index(self, sub: str, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /) -> int:
        """Return the lowest index in S where substring sub is found, such that sub is contained within S[start:end].

        Optional arguments start and end are interpreted as in slice notation.
        Raises ValueError when the substring is not found.
        """

    def isalnum(self) -> bool:
        """Return True if the string is an alpha-numeric string, False otherwise.

        A string is alpha-numeric if all characters in the string are alpha-numeric and
        there is at least one character in the string.
        """

    def isalpha(self) -> bool:
        """Return True if the string is an alphabetic string, False otherwise.

        A string is alphabetic if all characters in the string are alphabetic and there
        is at least one character in the string.
        """

    def isascii(self) -> bool:
        """Return True if all characters in the string are ASCII, False otherwise.

        ASCII characters have code points in the range U+0000-U+007F.
        Empty string is ASCII too.
        """

    def isdecimal(self) -> bool:
        """Return True if the string is a decimal string, False otherwise.

        A string is a decimal string if all characters in the string are decimal and
        there is at least one character in the string.
        """

    def isdigit(self) -> bool:
        """Return True if the string is a digit string, False otherwise.

        A string is a digit string if all characters in the string are digits and there
        is at least one character in the string.
        """

    def isidentifier(self) -> bool:
        """Return True if the string is a valid Python identifier, False otherwise.

        Call keyword.iskeyword(s) to test whether string s is a reserved identifier,
        such as "def" or "class".
        """

    def islower(self) -> bool:
        """Return True if the string is a lowercase string, False otherwise.

        A string is lowercase if all cased characters in the string are lowercase and
        there is at least one cased character in the string.
        """

    def isnumeric(self) -> bool:
        """Return True if the string is a numeric string, False otherwise.

        A string is numeric if all characters in the string are numeric and there is at
        least one character in the string.
        """

    def isprintable(self) -> bool:
        """Return True if all characters in the string are printable, False otherwise.

        A character is printable if repr() may use it in its output.
        """

    def isspace(self) -> bool:
        """Return True if the string is a whitespace string, False otherwise.

        A string is whitespace if all characters in the string are whitespace and there
        is at least one character in the string.
        """

    def istitle(self) -> bool:
        """Return True if the string is a title-cased string, False otherwise.

        In a title-cased string, upper- and title-case characters may only
        follow uncased characters and lowercase characters only cased ones.
        """

    def isupper(self) -> bool:
        """Return True if the string is an uppercase string, False otherwise.

        A string is uppercase if all cased characters in the string are uppercase and
        there is at least one cased character in the string.
        """

    @overload
    def join(self: LiteralString, iterable: Iterable[LiteralString], /) -> LiteralString:
        """Concatenate any number of strings.

        The string whose method is called is inserted in between each given string.
        The result is returned as a new string.

        Example: '.'.join(['ab', 'pq', 'rs']) -> 'ab.pq.rs'
        """

    @overload
    def join(self, iterable: Iterable[str], /) -> str: ...  # type: ignore[misc]
    @overload
    def ljust(self: LiteralString, width: SupportsIndex, fillchar: LiteralString = " ", /) -> LiteralString:
        """Return a left-justified string of length width.

        Padding is done using the specified fill character (default is a space).
        """

    @overload
    def ljust(self, width: SupportsIndex, fillchar: str = " ", /) -> str: ...  # type: ignore[misc]
    @overload
    def lower(self: LiteralString) -> LiteralString:
        """Return a copy of the string converted to lowercase."""

    @overload
    def lower(self) -> str: ...  # type: ignore[misc]
    @overload
    def lstrip(self: LiteralString, chars: LiteralString | None = None, /) -> LiteralString:
        """Return a copy of the string with leading whitespace removed.

        If chars is given and not None, remove characters in chars instead.
        """

    @overload
    def lstrip(self, chars: str | None = None, /) -> str: ...  # type: ignore[misc]
    @overload
    def partition(self: LiteralString, sep: LiteralString, /) -> tuple[LiteralString, LiteralString, LiteralString]:
        """Partition the string into three parts using the given separator.

        This will search for the separator in the string.  If the separator is found,
        returns a 3-tuple containing the part before the separator, the separator
        itself, and the part after it.

        If the separator is not found, returns a 3-tuple containing the original string
        and two empty strings.
        """

    @overload
    def partition(self, sep: str, /) -> tuple[str, str, str]: ...  # type: ignore[misc]
    if sys.version_info >= (3, 13):
        @overload
        def replace(self: LiteralString, old: LiteralString, new: LiteralString, /, count: SupportsIndex = -1) -> LiteralString:
            """Return a copy with all occurrences of substring old replaced by new.

              count
                Maximum number of occurrences to replace.
                -1 (the default value) means replace all occurrences.

            If the optional argument count is given, only the first count occurrences are
            replaced.
            """

        @overload
        def replace(self, old: str, new: str, /, count: SupportsIndex = -1) -> str: ...  # type: ignore[misc]
    else:
        @overload
        def replace(self: LiteralString, old: LiteralString, new: LiteralString, count: SupportsIndex = -1, /) -> LiteralString:
            """Return a copy with all occurrences of substring old replaced by new.

              count
                Maximum number of occurrences to replace.
                -1 (the default value) means replace all occurrences.

            If the optional argument count is given, only the first count occurrences are
            replaced.
            """

        @overload
        def replace(self, old: str, new: str, count: SupportsIndex = -1, /) -> str: ...  # type: ignore[misc]

    @overload
    def removeprefix(self: LiteralString, prefix: LiteralString, /) -> LiteralString:
        """Return a str with the given prefix string removed if present.

        If the string starts with the prefix string, return string[len(prefix):].
        Otherwise, return a copy of the original string.
        """

    @overload
    def removeprefix(self, prefix: str, /) -> str: ...  # type: ignore[misc]
    @overload
    def removesuffix(self: LiteralString, suffix: LiteralString, /) -> LiteralString:
        """Return a str with the given suffix string removed if present.

        If the string ends with the suffix string and that suffix is not empty,
        return string[:-len(suffix)]. Otherwise, return a copy of the original
        string.
        """

    @overload
    def removesuffix(self, suffix: str, /) -> str: ...  # type: ignore[misc]
    def rfind(self, sub: str, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /) -> int:
        """Return the highest index in S where substring sub is found, such that sub is contained within S[start:end].

        Optional arguments start and end are interpreted as in slice notation.
        Return -1 on failure.
        """

    def rindex(self, sub: str, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /) -> int:
        """Return the highest index in S where substring sub is found, such that sub is contained within S[start:end].

        Optional arguments start and end are interpreted as in slice notation.
        Raises ValueError when the substring is not found.
        """

    @overload
    def rjust(self: LiteralString, width: SupportsIndex, fillchar: LiteralString = " ", /) -> LiteralString:
        """Return a right-justified string of length width.

        Padding is done using the specified fill character (default is a space).
        """

    @overload
    def rjust(self, width: SupportsIndex, fillchar: str = " ", /) -> str: ...  # type: ignore[misc]
    @overload
    def rpartition(self: LiteralString, sep: LiteralString, /) -> tuple[LiteralString, LiteralString, LiteralString]:
        """Partition the string into three parts using the given separator.

        This will search for the separator in the string, starting at the end. If
        the separator is found, returns a 3-tuple containing the part before the
        separator, the separator itself, and the part after it.

        If the separator is not found, returns a 3-tuple containing two empty strings
        and the original string.
        """

    @overload
    def rpartition(self, sep: str, /) -> tuple[str, str, str]: ...  # type: ignore[misc]
    @overload
    def rsplit(self: LiteralString, sep: LiteralString | None = None, maxsplit: SupportsIndex = -1) -> list[LiteralString]:
        """Return a list of the substrings in the string, using sep as the separator string.

          sep
            The separator used to split the string.

            When set to None (the default value), will split on any whitespace
            character (including \\n \\r \\t \\f and spaces) and will discard
            empty strings from the result.
          maxsplit
            Maximum number of splits.
            -1 (the default value) means no limit.

        Splitting starts at the end of the string and works to the front.
        """

    @overload
    def rsplit(self, sep: str | None = None, maxsplit: SupportsIndex = -1) -> list[str]: ...  # type: ignore[misc]
    @overload
    def rstrip(self: LiteralString, chars: LiteralString | None = None, /) -> LiteralString:
        """Return a copy of the string with trailing whitespace removed.

        If chars is given and not None, remove characters in chars instead.
        """

    @overload
    def rstrip(self, chars: str | None = None, /) -> str: ...  # type: ignore[misc]
    @overload
    def split(self: LiteralString, sep: LiteralString | None = None, maxsplit: SupportsIndex = -1) -> list[LiteralString]:
        """Return a list of the substrings in the string, using sep as the separator string.

          sep
            The separator used to split the string.

            When set to None (the default value), will split on any whitespace
            character (including \\n \\r \\t \\f and spaces) and will discard
            empty strings from the result.
          maxsplit
            Maximum number of splits.
            -1 (the default value) means no limit.

        Splitting starts at the front of the string and works to the end.

        Note, str.split() is mainly useful for data that has been intentionally
        delimited.  With natural text that includes punctuation, consider using
        the regular expression module.
        """

    @overload
    def split(self, sep: str | None = None, maxsplit: SupportsIndex = -1) -> list[str]: ...  # type: ignore[misc]
    @overload
    def splitlines(self: LiteralString, keepends: bool = False) -> list[LiteralString]:
        """Return a list of the lines in the string, breaking at line boundaries.

        Line breaks are not included in the resulting list unless keepends is given and
        true.
        """

    @overload
    def splitlines(self, keepends: bool = False) -> list[str]: ...  # type: ignore[misc]
    def startswith(
        self, prefix: str | tuple[str, ...], start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> bool:
        """Return True if the string starts with the specified prefix, False otherwise.

        prefix
          A string or a tuple of strings to try.
        start
          Optional start position. Default: start of the string.
        end
          Optional stop position. Default: end of the string.
        """

    @overload
    def strip(self: LiteralString, chars: LiteralString | None = None, /) -> LiteralString:
        """Return a copy of the string with leading and trailing whitespace removed.

        If chars is given and not None, remove characters in chars instead.
        """

    @overload
    def strip(self, chars: str | None = None, /) -> str: ...  # type: ignore[misc]
    @overload
    def swapcase(self: LiteralString) -> LiteralString:
        """Convert uppercase characters to lowercase and lowercase characters to uppercase."""

    @overload
    def swapcase(self) -> str: ...  # type: ignore[misc]
    @overload
    def title(self: LiteralString) -> LiteralString:
        """Return a version of the string where each word is titlecased.

        More specifically, words start with uppercased characters and all remaining
        cased characters have lower case.
        """

    @overload
    def title(self) -> str: ...  # type: ignore[misc]
    def translate(self, table: _TranslateTable, /) -> str:
        """Replace each character in the string using the given translation table.

          table
            Translation table, which must be a mapping of Unicode ordinals to
            Unicode ordinals, strings, or None.

        The table must implement lookup/indexing via __getitem__, for instance a
        dictionary or list.  If this operation raises LookupError, the character is
        left untouched.  Characters mapped to None are deleted.
        """

    @overload
    def upper(self: LiteralString) -> LiteralString:
        """Return a copy of the string converted to uppercase."""

    @overload
    def upper(self) -> str: ...  # type: ignore[misc]
    @overload
    def zfill(self: LiteralString, width: SupportsIndex, /) -> LiteralString:
        """Pad a numeric string with zeros on the left, to fill a field of the given width.

        The string is never truncated.
        """

    @overload
    def zfill(self, width: SupportsIndex, /) -> str: ...  # type: ignore[misc]
    @staticmethod
    @overload
    def maketrans(x: dict[int, _T] | dict[str, _T] | dict[str | int, _T], /) -> dict[int, _T]:
        """Return a translation table usable for str.translate().

        If there is only one argument, it must be a dictionary mapping Unicode
        ordinals (integers) or characters to Unicode ordinals, strings or None.
        Character keys will be then converted to ordinals.
        If there are two arguments, they must be strings of equal length, and
        in the resulting dictionary, each character in x will be mapped to the
        character at the same position in y. If there is a third argument, it
        must be a string, whose characters will be mapped to None in the result.
        """

    @staticmethod
    @overload
    def maketrans(x: str, y: str, /) -> dict[int, int]: ...
    @staticmethod
    @overload
    def maketrans(x: str, y: str, z: str, /) -> dict[int, int | None]: ...
    @overload
    def __add__(self: LiteralString, value: LiteralString, /) -> LiteralString:
        """Return self+value."""

    @overload
    def __add__(self, value: str, /) -> str: ...  # type: ignore[misc]
    # Incompatible with Sequence.__contains__
    def __contains__(self, key: str, /) -> bool:  # type: ignore[override]
        """Return bool(key in self)."""

    def __eq__(self, value: object, /) -> bool: ...
    def __ge__(self, value: str, /) -> bool: ...
    @overload
    def __getitem__(self: LiteralString, key: SupportsIndex | slice, /) -> LiteralString:
        """Return self[key]."""

    @overload
    def __getitem__(self, key: SupportsIndex | slice, /) -> str: ...  # type: ignore[misc]
    def __gt__(self, value: str, /) -> bool: ...
    def __hash__(self) -> int: ...
    @overload
    def __iter__(self: LiteralString) -> Iterator[LiteralString]:
        """Implement iter(self)."""

    @overload
    def __iter__(self) -> Iterator[str]: ...  # type: ignore[misc]
    def __le__(self, value: str, /) -> bool: ...
    def __len__(self) -> int:
        """Return len(self)."""

    def __lt__(self, value: str, /) -> bool: ...
    @overload
    def __mod__(self: LiteralString, value: LiteralString | tuple[LiteralString, ...], /) -> LiteralString:
        """Return self%value."""

    @overload
    def __mod__(self, value: Any, /) -> str: ...
    @overload
    def __mul__(self: LiteralString, value: SupportsIndex, /) -> LiteralString:
        """Return self*value."""

    @overload
    def __mul__(self, value: SupportsIndex, /) -> str: ...  # type: ignore[misc]
    def __ne__(self, value: object, /) -> bool: ...
    @overload
    def __rmul__(self: LiteralString, value: SupportsIndex, /) -> LiteralString:
        """Return value*self."""

    @overload
    def __rmul__(self, value: SupportsIndex, /) -> str: ...  # type: ignore[misc]
    def __getnewargs__(self) -> tuple[str]: ...
    def __format__(self, format_spec: str, /) -> str:
        """Return a formatted version of the string as described by format_spec."""

@disjoint_base
class bytes(Sequence[int]):
    """bytes(iterable_of_ints) -> bytes
    bytes(string, encoding[, errors]) -> bytes
    bytes(bytes_or_buffer) -> immutable copy of bytes_or_buffer
    bytes(int) -> bytes object of size given by the parameter initialized with null bytes
    bytes() -> empty bytes object

    Construct an immutable array of bytes from:
      - an iterable yielding integers in range(256)
      - a text string encoded using the specified encoding
      - any object implementing the buffer API.
      - an integer
    """

    @overload
    def __new__(cls, o: Iterable[SupportsIndex] | SupportsIndex | SupportsBytes | ReadableBuffer, /) -> Self: ...
    @overload
    def __new__(cls, string: str, /, encoding: str, errors: str = "strict") -> Self: ...
    @overload
    def __new__(cls) -> Self: ...
    def capitalize(self) -> bytes:
        """B.capitalize() -> copy of B

        Return a copy of B with only its first character capitalized (ASCII)
        and the rest lower-cased.
        """

    def center(self, width: SupportsIndex, fillchar: bytes = b" ", /) -> bytes:
        """Return a centered string of length width.

        Padding is done using the specified fill character.
        """

    def count(
        self, sub: ReadableBuffer | SupportsIndex, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> int:
        """Return the number of non-overlapping occurrences of subsection 'sub' in bytes B[start:end].

        start
          Optional start position. Default: start of the bytes.
        end
          Optional stop position. Default: end of the bytes.
        """

    def decode(self, encoding: str = "utf-8", errors: str = "strict") -> str:
        """Decode the bytes using the codec registered for encoding.

        encoding
          The encoding with which to decode the bytes.
        errors
          The error handling scheme to use for the handling of decoding errors.
          The default is 'strict' meaning that decoding errors raise a
          UnicodeDecodeError. Other possible values are 'ignore' and 'replace'
          as well as any other name registered with codecs.register_error that
          can handle UnicodeDecodeErrors.
        """

    def endswith(
        self,
        suffix: ReadableBuffer | tuple[ReadableBuffer, ...],
        start: SupportsIndex | None = None,
        end: SupportsIndex | None = None,
        /,
    ) -> bool:
        """Return True if the bytes ends with the specified suffix, False otherwise.

        suffix
          A bytes or a tuple of bytes to try.
        start
          Optional start position. Default: start of the bytes.
        end
          Optional stop position. Default: end of the bytes.
        """

    def expandtabs(self, tabsize: SupportsIndex = 8) -> bytes:
        """Return a copy where all tab characters are expanded using spaces.

        If tabsize is not given, a tab size of 8 characters is assumed.
        """

    def find(
        self, sub: ReadableBuffer | SupportsIndex, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> int:
        """Return the lowest index in B where subsection 'sub' is found, such that 'sub' is contained within B[start,end].

          start
            Optional start position. Default: start of the bytes.
          end
            Optional stop position. Default: end of the bytes.

        Return -1 on failure.
        """

    def hex(self, sep: str | bytes = ..., bytes_per_sep: SupportsIndex = 1) -> str:
        """Create a string of hexadecimal numbers from a bytes object.

          sep
            An optional single character or byte to separate hex bytes.
          bytes_per_sep
            How many bytes between separators.  Positive values count from the
            right, negative values count from the left.

        Example:
        >>> value = b'\\xb9\\x01\\xef'
        >>> value.hex()
        'b901ef'
        >>> value.hex(':')
        'b9:01:ef'
        >>> value.hex(':', 2)
        'b9:01ef'
        >>> value.hex(':', -2)
        'b901:ef'
        """

    def index(
        self, sub: ReadableBuffer | SupportsIndex, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> int:
        """Return the lowest index in B where subsection 'sub' is found, such that 'sub' is contained within B[start,end].

          start
            Optional start position. Default: start of the bytes.
          end
            Optional stop position. Default: end of the bytes.

        Raise ValueError if the subsection is not found.
        """

    def isalnum(self) -> bool:
        """B.isalnum() -> bool

        Return True if all characters in B are alphanumeric
        and there is at least one character in B, False otherwise.
        """

    def isalpha(self) -> bool:
        """B.isalpha() -> bool

        Return True if all characters in B are alphabetic
        and there is at least one character in B, False otherwise.
        """

    def isascii(self) -> bool:
        """B.isascii() -> bool

        Return True if B is empty or all characters in B are ASCII,
        False otherwise.
        """

    def isdigit(self) -> bool:
        """B.isdigit() -> bool

        Return True if all characters in B are digits
        and there is at least one character in B, False otherwise.
        """

    def islower(self) -> bool:
        """B.islower() -> bool

        Return True if all cased characters in B are lowercase and there is
        at least one cased character in B, False otherwise.
        """

    def isspace(self) -> bool:
        """B.isspace() -> bool

        Return True if all characters in B are whitespace
        and there is at least one character in B, False otherwise.
        """

    def istitle(self) -> bool:
        """B.istitle() -> bool

        Return True if B is a titlecased string and there is at least one
        character in B, i.e. uppercase characters may only follow uncased
        characters and lowercase characters only cased ones. Return False
        otherwise.
        """

    def isupper(self) -> bool:
        """B.isupper() -> bool

        Return True if all cased characters in B are uppercase and there is
        at least one cased character in B, False otherwise.
        """

    def join(self, iterable_of_bytes: Iterable[ReadableBuffer], /) -> bytes:
        """Concatenate any number of bytes objects.

        The bytes whose method is called is inserted in between each pair.

        The result is returned as a new bytes object.

        Example: b'.'.join([b'ab', b'pq', b'rs']) -> b'ab.pq.rs'.
        """

    def ljust(self, width: SupportsIndex, fillchar: bytes | bytearray = b" ", /) -> bytes:
        """Return a left-justified string of length width.

        Padding is done using the specified fill character.
        """

    def lower(self) -> bytes:
        """B.lower() -> copy of B

        Return a copy of B with all ASCII characters converted to lowercase.
        """

    def lstrip(self, bytes: ReadableBuffer | None = None, /) -> bytes:
        """Strip leading bytes contained in the argument.

        If the argument is omitted or None, strip leading  ASCII whitespace.
        """

    def partition(self, sep: ReadableBuffer, /) -> tuple[bytes, bytes, bytes]:
        """Partition the bytes into three parts using the given separator.

        This will search for the separator sep in the bytes. If the separator is found,
        returns a 3-tuple containing the part before the separator, the separator
        itself, and the part after it.

        If the separator is not found, returns a 3-tuple containing the original bytes
        object and two empty bytes objects.
        """

    def replace(self, old: ReadableBuffer, new: ReadableBuffer, count: SupportsIndex = -1, /) -> bytes:
        """Return a copy with all occurrences of substring old replaced by new.

          count
            Maximum number of occurrences to replace.
            -1 (the default value) means replace all occurrences.

        If the optional argument count is given, only the first count occurrences are
        replaced.
        """

    def removeprefix(self, prefix: ReadableBuffer, /) -> bytes:
        """Return a bytes object with the given prefix string removed if present.

        If the bytes starts with the prefix string, return bytes[len(prefix):].
        Otherwise, return a copy of the original bytes.
        """

    def removesuffix(self, suffix: ReadableBuffer, /) -> bytes:
        """Return a bytes object with the given suffix string removed if present.

        If the bytes ends with the suffix string and that suffix is not empty,
        return bytes[:-len(prefix)].  Otherwise, return a copy of the original
        bytes.
        """

    def rfind(
        self, sub: ReadableBuffer | SupportsIndex, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> int:
        """Return the highest index in B where subsection 'sub' is found, such that 'sub' is contained within B[start,end].

          start
            Optional start position. Default: start of the bytes.
          end
            Optional stop position. Default: end of the bytes.

        Return -1 on failure.
        """

    def rindex(
        self, sub: ReadableBuffer | SupportsIndex, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> int:
        """Return the highest index in B where subsection 'sub' is found, such that 'sub' is contained within B[start,end].

          start
            Optional start position. Default: start of the bytes.
          end
            Optional stop position. Default: end of the bytes.

        Raise ValueError if the subsection is not found.
        """

    def rjust(self, width: SupportsIndex, fillchar: bytes | bytearray = b" ", /) -> bytes:
        """Return a right-justified string of length width.

        Padding is done using the specified fill character.
        """

    def rpartition(self, sep: ReadableBuffer, /) -> tuple[bytes, bytes, bytes]:
        """Partition the bytes into three parts using the given separator.

        This will search for the separator sep in the bytes, starting at the end. If
        the separator is found, returns a 3-tuple containing the part before the
        separator, the separator itself, and the part after it.

        If the separator is not found, returns a 3-tuple containing two empty bytes
        objects and the original bytes object.
        """

    def rsplit(self, sep: ReadableBuffer | None = None, maxsplit: SupportsIndex = -1) -> list[bytes]:
        """Return a list of the sections in the bytes, using sep as the delimiter.

          sep
            The delimiter according which to split the bytes.
            None (the default value) means split on ASCII whitespace characters
            (space, tab, return, newline, formfeed, vertical tab).
          maxsplit
            Maximum number of splits to do.
            -1 (the default value) means no limit.

        Splitting is done starting at the end of the bytes and working to the front.
        """

    def rstrip(self, bytes: ReadableBuffer | None = None, /) -> bytes:
        """Strip trailing bytes contained in the argument.

        If the argument is omitted or None, strip trailing ASCII whitespace.
        """

    def split(self, sep: ReadableBuffer | None = None, maxsplit: SupportsIndex = -1) -> list[bytes]:
        """Return a list of the sections in the bytes, using sep as the delimiter.

        sep
          The delimiter according which to split the bytes.
          None (the default value) means split on ASCII whitespace characters
          (space, tab, return, newline, formfeed, vertical tab).
        maxsplit
          Maximum number of splits to do.
          -1 (the default value) means no limit.
        """

    def splitlines(self, keepends: bool = False) -> list[bytes]:
        """Return a list of the lines in the bytes, breaking at line boundaries.

        Line breaks are not included in the resulting list unless keepends is given and
        true.
        """

    def startswith(
        self,
        prefix: ReadableBuffer | tuple[ReadableBuffer, ...],
        start: SupportsIndex | None = None,
        end: SupportsIndex | None = None,
        /,
    ) -> bool:
        """Return True if the bytes starts with the specified prefix, False otherwise.

        prefix
          A bytes or a tuple of bytes to try.
        start
          Optional start position. Default: start of the bytes.
        end
          Optional stop position. Default: end of the bytes.
        """

    def strip(self, bytes: ReadableBuffer | None = None, /) -> bytes:
        """Strip leading and trailing bytes contained in the argument.

        If the argument is omitted or None, strip leading and trailing ASCII whitespace.
        """

    def swapcase(self) -> bytes:
        """B.swapcase() -> copy of B

        Return a copy of B with uppercase ASCII characters converted
        to lowercase ASCII and vice versa.
        """

    def title(self) -> bytes:
        """B.title() -> copy of B

        Return a titlecased version of B, i.e. ASCII words start with uppercase
        characters, all remaining cased characters have lowercase.
        """

    def translate(self, table: ReadableBuffer | None, /, delete: ReadableBuffer = b"") -> bytes:
        """Return a copy with each character mapped by the given translation table.

          table
            Translation table, which must be a bytes object of length 256.

        All characters occurring in the optional argument delete are removed.
        The remaining characters are mapped through the given translation table.
        """

    def upper(self) -> bytes:
        """B.upper() -> copy of B

        Return a copy of B with all ASCII characters converted to uppercase.
        """

    def zfill(self, width: SupportsIndex, /) -> bytes:
        """Pad a numeric string with zeros on the left, to fill a field of the given width.

        The original string is never truncated.
        """
    if sys.version_info >= (3, 14):
        @classmethod
        def fromhex(cls, string: str | ReadableBuffer, /) -> Self:
            """Create a bytes object from a string of hexadecimal numbers.

            Spaces between two numbers are accepted.
            Example: bytes.fromhex('B9 01EF') -> b'\\\\xb9\\\\x01\\\\xef'.
            """
    else:
        @classmethod
        def fromhex(cls, string: str, /) -> Self:
            """Create a bytes object from a string of hexadecimal numbers.

            Spaces between two numbers are accepted.
            Example: bytes.fromhex('B9 01EF') -> b'\\\\xb9\\\\x01\\\\xef'.
            """

    @staticmethod
    def maketrans(frm: ReadableBuffer, to: ReadableBuffer, /) -> bytes:
        """Return a translation table usable for the bytes or bytearray translate method.

        The returned table will be one where each byte in frm is mapped to the byte at
        the same position in to.

        The bytes objects frm and to must be of the same length.
        """

    def __len__(self) -> int:
        """Return len(self)."""

    def __iter__(self) -> Iterator[int]:
        """Implement iter(self)."""

    def __hash__(self) -> int: ...
    @overload
    def __getitem__(self, key: SupportsIndex, /) -> int:
        """Return self[key]."""

    @overload
    def __getitem__(self, key: slice, /) -> bytes: ...
    def __add__(self, value: ReadableBuffer, /) -> bytes:
        """Return self+value."""

    def __mul__(self, value: SupportsIndex, /) -> bytes:
        """Return self*value."""

    def __rmul__(self, value: SupportsIndex, /) -> bytes:
        """Return value*self."""

    def __mod__(self, value: Any, /) -> bytes:
        """Return self%value."""
    # Incompatible with Sequence.__contains__
    def __contains__(self, key: SupportsIndex | ReadableBuffer, /) -> bool:  # type: ignore[override]
        """Return bool(key in self)."""

    def __eq__(self, value: object, /) -> bool: ...
    def __ne__(self, value: object, /) -> bool: ...
    def __lt__(self, value: bytes, /) -> bool: ...
    def __le__(self, value: bytes, /) -> bool: ...
    def __gt__(self, value: bytes, /) -> bool: ...
    def __ge__(self, value: bytes, /) -> bool: ...
    def __getnewargs__(self) -> tuple[bytes]: ...
    if sys.version_info >= (3, 11):
        def __bytes__(self) -> bytes:
            """Convert this value to exact type bytes."""

    def __buffer__(self, flags: int, /) -> memoryview:
        """Return a buffer object that exposes the underlying memory of the object."""

@disjoint_base
class bytearray(MutableSequence[int]):
    """bytearray(iterable_of_ints) -> bytearray
    bytearray(string, encoding[, errors]) -> bytearray
    bytearray(bytes_or_buffer) -> mutable copy of bytes_or_buffer
    bytearray(int) -> bytes array of size given by the parameter initialized with null bytes
    bytearray() -> empty bytes array

    Construct a mutable bytearray object from:
      - an iterable yielding integers in range(256)
      - a text string encoded using the specified encoding
      - a bytes or a buffer object
      - any object implementing the buffer API.
      - an integer
    """

    @overload
    def __init__(self) -> None: ...
    @overload
    def __init__(self, ints: Iterable[SupportsIndex] | SupportsIndex | ReadableBuffer, /) -> None: ...
    @overload
    def __init__(self, string: str, /, encoding: str, errors: str = "strict") -> None: ...
    def append(self, item: SupportsIndex, /) -> None:
        """Append a single item to the end of the bytearray.

        item
          The item to be appended.
        """

    def capitalize(self) -> bytearray:
        """B.capitalize() -> copy of B

        Return a copy of B with only its first character capitalized (ASCII)
        and the rest lower-cased.
        """

    def center(self, width: SupportsIndex, fillchar: bytes = b" ", /) -> bytearray:
        """Return a centered string of length width.

        Padding is done using the specified fill character.
        """

    def count(
        self, sub: ReadableBuffer | SupportsIndex, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> int:
        """Return the number of non-overlapping occurrences of subsection 'sub' in bytes B[start:end].

        start
          Optional start position. Default: start of the bytes.
        end
          Optional stop position. Default: end of the bytes.
        """

    def copy(self) -> bytearray:
        """Return a copy of B."""

    def decode(self, encoding: str = "utf-8", errors: str = "strict") -> str:
        """Decode the bytearray using the codec registered for encoding.

        encoding
          The encoding with which to decode the bytearray.
        errors
          The error handling scheme to use for the handling of decoding errors.
          The default is 'strict' meaning that decoding errors raise a
          UnicodeDecodeError. Other possible values are 'ignore' and 'replace'
          as well as any other name registered with codecs.register_error that
          can handle UnicodeDecodeErrors.
        """

    def endswith(
        self,
        suffix: ReadableBuffer | tuple[ReadableBuffer, ...],
        start: SupportsIndex | None = None,
        end: SupportsIndex | None = None,
        /,
    ) -> bool:
        """Return True if the bytearray ends with the specified suffix, False otherwise.

        suffix
          A bytes or a tuple of bytes to try.
        start
          Optional start position. Default: start of the bytearray.
        end
          Optional stop position. Default: end of the bytearray.
        """

    def expandtabs(self, tabsize: SupportsIndex = 8) -> bytearray:
        """Return a copy where all tab characters are expanded using spaces.

        If tabsize is not given, a tab size of 8 characters is assumed.
        """

    def extend(self, iterable_of_ints: Iterable[SupportsIndex], /) -> None:
        """Append all the items from the iterator or sequence to the end of the bytearray.

        iterable_of_ints
          The iterable of items to append.
        """

    def find(
        self, sub: ReadableBuffer | SupportsIndex, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> int:
        """Return the lowest index in B where subsection 'sub' is found, such that 'sub' is contained within B[start:end].

          start
            Optional start position. Default: start of the bytes.
          end
            Optional stop position. Default: end of the bytes.

        Return -1 on failure.
        """

    def hex(self, sep: str | bytes = ..., bytes_per_sep: SupportsIndex = 1) -> str:
        """Create a string of hexadecimal numbers from a bytearray object.

          sep
            An optional single character or byte to separate hex bytes.
          bytes_per_sep
            How many bytes between separators.  Positive values count from the
            right, negative values count from the left.

        Example:
        >>> value = bytearray([0xb9, 0x01, 0xef])
        >>> value.hex()
        'b901ef'
        >>> value.hex(':')
        'b9:01:ef'
        >>> value.hex(':', 2)
        'b9:01ef'
        >>> value.hex(':', -2)
        'b901:ef'
        """

    def index(
        self, sub: ReadableBuffer | SupportsIndex, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> int:
        """Return the lowest index in B where subsection 'sub' is found, such that 'sub' is contained within B[start:end].

          start
            Optional start position. Default: start of the bytes.
          end
            Optional stop position. Default: end of the bytes.

        Raise ValueError if the subsection is not found.
        """

    def insert(self, index: SupportsIndex, item: SupportsIndex, /) -> None:
        """Insert a single item into the bytearray before the given index.

        index
          The index where the value is to be inserted.
        item
          The item to be inserted.
        """

    def isalnum(self) -> bool:
        """B.isalnum() -> bool

        Return True if all characters in B are alphanumeric
        and there is at least one character in B, False otherwise.
        """

    def isalpha(self) -> bool:
        """B.isalpha() -> bool

        Return True if all characters in B are alphabetic
        and there is at least one character in B, False otherwise.
        """

    def isascii(self) -> bool:
        """B.isascii() -> bool

        Return True if B is empty or all characters in B are ASCII,
        False otherwise.
        """

    def isdigit(self) -> bool:
        """B.isdigit() -> bool

        Return True if all characters in B are digits
        and there is at least one character in B, False otherwise.
        """

    def islower(self) -> bool:
        """B.islower() -> bool

        Return True if all cased characters in B are lowercase and there is
        at least one cased character in B, False otherwise.
        """

    def isspace(self) -> bool:
        """B.isspace() -> bool

        Return True if all characters in B are whitespace
        and there is at least one character in B, False otherwise.
        """

    def istitle(self) -> bool:
        """B.istitle() -> bool

        Return True if B is a titlecased string and there is at least one
        character in B, i.e. uppercase characters may only follow uncased
        characters and lowercase characters only cased ones. Return False
        otherwise.
        """

    def isupper(self) -> bool:
        """B.isupper() -> bool

        Return True if all cased characters in B are uppercase and there is
        at least one cased character in B, False otherwise.
        """

    def join(self, iterable_of_bytes: Iterable[ReadableBuffer], /) -> bytearray:
        """Concatenate any number of bytes/bytearray objects.

        The bytearray whose method is called is inserted in between each pair.

        The result is returned as a new bytearray object.
        """

    def ljust(self, width: SupportsIndex, fillchar: bytes | bytearray = b" ", /) -> bytearray:
        """Return a left-justified string of length width.

        Padding is done using the specified fill character.
        """

    def lower(self) -> bytearray:
        """B.lower() -> copy of B

        Return a copy of B with all ASCII characters converted to lowercase.
        """

    def lstrip(self, bytes: ReadableBuffer | None = None, /) -> bytearray:
        """Strip leading bytes contained in the argument.

        If the argument is omitted or None, strip leading ASCII whitespace.
        """

    def partition(self, sep: ReadableBuffer, /) -> tuple[bytearray, bytearray, bytearray]:
        """Partition the bytearray into three parts using the given separator.

        This will search for the separator sep in the bytearray. If the separator is
        found, returns a 3-tuple containing the part before the separator, the
        separator itself, and the part after it as new bytearray objects.

        If the separator is not found, returns a 3-tuple containing the copy of the
        original bytearray object and two empty bytearray objects.
        """

    def pop(self, index: int = -1, /) -> int:
        """Remove and return a single item from B.

          index
            The index from where to remove the item.
            -1 (the default value) means remove the last item.

        If no index argument is given, will pop the last item.
        """

    def remove(self, value: int, /) -> None:
        """Remove the first occurrence of a value in the bytearray.

        value
          The value to remove.
        """

    def removeprefix(self, prefix: ReadableBuffer, /) -> bytearray:
        """Return a bytearray with the given prefix string removed if present.

        If the bytearray starts with the prefix string, return
        bytearray[len(prefix):].  Otherwise, return a copy of the original
        bytearray.
        """

    def removesuffix(self, suffix: ReadableBuffer, /) -> bytearray:
        """Return a bytearray with the given suffix string removed if present.

        If the bytearray ends with the suffix string and that suffix is not
        empty, return bytearray[:-len(suffix)].  Otherwise, return a copy of
        the original bytearray.
        """

    def replace(self, old: ReadableBuffer, new: ReadableBuffer, count: SupportsIndex = -1, /) -> bytearray:
        """Return a copy with all occurrences of substring old replaced by new.

          count
            Maximum number of occurrences to replace.
            -1 (the default value) means replace all occurrences.

        If the optional argument count is given, only the first count occurrences are
        replaced.
        """

    def rfind(
        self, sub: ReadableBuffer | SupportsIndex, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> int:
        """Return the highest index in B where subsection 'sub' is found, such that 'sub' is contained within B[start:end].

          start
            Optional start position. Default: start of the bytes.
          end
            Optional stop position. Default: end of the bytes.

        Return -1 on failure.
        """

    def rindex(
        self, sub: ReadableBuffer | SupportsIndex, start: SupportsIndex | None = None, end: SupportsIndex | None = None, /
    ) -> int:
        """Return the highest index in B where subsection 'sub' is found, such that 'sub' is contained within B[start:end].

          start
            Optional start position. Default: start of the bytes.
          end
            Optional stop position. Default: end of the bytes.

        Raise ValueError if the subsection is not found.
        """

    def rjust(self, width: SupportsIndex, fillchar: bytes | bytearray = b" ", /) -> bytearray:
        """Return a right-justified string of length width.

        Padding is done using the specified fill character.
        """

    def rpartition(self, sep: ReadableBuffer, /) -> tuple[bytearray, bytearray, bytearray]:
        """Partition the bytearray into three parts using the given separator.

        This will search for the separator sep in the bytearray, starting at the end.
        If the separator is found, returns a 3-tuple containing the part before the
        separator, the separator itself, and the part after it as new bytearray
        objects.

        If the separator is not found, returns a 3-tuple containing two empty bytearray
        objects and the copy of the original bytearray object.
        """

    def rsplit(self, sep: ReadableBuffer | None = None, maxsplit: SupportsIndex = -1) -> list[bytearray]:
        """Return a list of the sections in the bytearray, using sep as the delimiter.

          sep
            The delimiter according which to split the bytearray.
            None (the default value) means split on ASCII whitespace characters
            (space, tab, return, newline, formfeed, vertical tab).
          maxsplit
            Maximum number of splits to do.
            -1 (the default value) means no limit.

        Splitting is done starting at the end of the bytearray and working to the front.
        """

    def rstrip(self, bytes: ReadableBuffer | None = None, /) -> bytearray:
        """Strip trailing bytes contained in the argument.

        If the argument is omitted or None, strip trailing ASCII whitespace.
        """

    def split(self, sep: ReadableBuffer | None = None, maxsplit: SupportsIndex = -1) -> list[bytearray]:
        """Return a list of the sections in the bytearray, using sep as the delimiter.

        sep
          The delimiter according which to split the bytearray.
          None (the default value) means split on ASCII whitespace characters
          (space, tab, return, newline, formfeed, vertical tab).
        maxsplit
          Maximum number of splits to do.
          -1 (the default value) means no limit.
        """

    def splitlines(self, keepends: bool = False) -> list[bytearray]:
        """Return a list of the lines in the bytearray, breaking at line boundaries.

        Line breaks are not included in the resulting list unless keepends is given and
        true.
        """

    def startswith(
        self,
        prefix: ReadableBuffer | tuple[ReadableBuffer, ...],
        start: SupportsIndex | None = None,
        end: SupportsIndex | None = None,
        /,
    ) -> bool:
        """Return True if the bytearray starts with the specified prefix, False otherwise.

        prefix
          A bytes or a tuple of bytes to try.
        start
          Optional start position. Default: start of the bytearray.
        end
          Optional stop position. Default: end of the bytearray.
        """

    def strip(self, bytes: ReadableBuffer | None = None, /) -> bytearray:
        """Strip leading and trailing bytes contained in the argument.

        If the argument is omitted or None, strip leading and trailing ASCII whitespace.
        """

    def swapcase(self) -> bytearray:
        """B.swapcase() -> copy of B

        Return a copy of B with uppercase ASCII characters converted
        to lowercase ASCII and vice versa.
        """

    def title(self) -> bytearray:
        """B.title() -> copy of B

        Return a titlecased version of B, i.e. ASCII words start with uppercase
        characters, all remaining cased characters have lowercase.
        """

    def translate(self, table: ReadableBuffer | None, /, delete: bytes = b"") -> bytearray:
        """Return a copy with each character mapped by the given translation table.

          table
            Translation table, which must be a bytes object of length 256.

        All characters occurring in the optional argument delete are removed.
        The remaining characters are mapped through the given translation table.
        """

    def upper(self) -> bytearray:
        """B.upper() -> copy of B

        Return a copy of B with all ASCII characters converted to uppercase.
        """

    def zfill(self, width: SupportsIndex, /) -> bytearray:
        """Pad a numeric string with zeros on the left, to fill a field of the given width.

        The original string is never truncated.
        """
    if sys.version_info >= (3, 14):
        @classmethod
        def fromhex(cls, string: str | ReadableBuffer, /) -> Self:
            """Create a bytearray object from a string of hexadecimal numbers.

            Spaces between two numbers are accepted.
            Example: bytearray.fromhex('B9 01EF') -> bytearray(b'\\\\xb9\\\\x01\\\\xef')
            """
    else:
        @classmethod
        def fromhex(cls, string: str, /) -> Self:
            """Create a bytearray object from a string of hexadecimal numbers.

            Spaces between two numbers are accepted.
            Example: bytearray.fromhex('B9 01EF') -> bytearray(b'\\\\xb9\\\\x01\\\\xef')
            """

    @staticmethod
    def maketrans(frm: ReadableBuffer, to: ReadableBuffer, /) -> bytes:
        """Return a translation table usable for the bytes or bytearray translate method.

        The returned table will be one where each byte in frm is mapped to the byte at
        the same position in to.

        The bytes objects frm and to must be of the same length.
        """

    def __len__(self) -> int:
        """Return len(self)."""

    def __iter__(self) -> Iterator[int]:
        """Implement iter(self)."""
    __hash__: ClassVar[None]  # type: ignore[assignment]
    @overload
    def __getitem__(self, key: SupportsIndex, /) -> int:
        """Return self[key]."""

    @overload
    def __getitem__(self, key: slice, /) -> bytearray: ...
    @overload
    def __setitem__(self, key: SupportsIndex, value: SupportsIndex, /) -> None:
        """Set self[key] to value."""

    @overload
    def __setitem__(self, key: slice, value: Iterable[SupportsIndex] | bytes, /) -> None: ...
    def __delitem__(self, key: SupportsIndex | slice, /) -> None:
        """Delete self[key]."""

    def __add__(self, value: ReadableBuffer, /) -> bytearray:
        """Return self+value."""
    # The superclass wants us to accept Iterable[int], but that fails at runtime.
    def __iadd__(self, value: ReadableBuffer, /) -> Self:  # type: ignore[override]
        """Implement self+=value."""

    def __mul__(self, value: SupportsIndex, /) -> bytearray:
        """Return self*value."""

    def __rmul__(self, value: SupportsIndex, /) -> bytearray:
        """Return value*self."""

    def __imul__(self, value: SupportsIndex, /) -> Self:
        """Implement self*=value."""

    def __mod__(self, value: Any, /) -> bytes:
        """Return self%value."""
    # Incompatible with Sequence.__contains__
    def __contains__(self, key: SupportsIndex | ReadableBuffer, /) -> bool:  # type: ignore[override]
        """Return bool(key in self)."""

    def __eq__(self, value: object, /) -> bool: ...
    def __ne__(self, value: object, /) -> bool: ...
    def __lt__(self, value: ReadableBuffer, /) -> bool: ...
    def __le__(self, value: ReadableBuffer, /) -> bool: ...
    def __gt__(self, value: ReadableBuffer, /) -> bool: ...
    def __ge__(self, value: ReadableBuffer, /) -> bool: ...
    def __alloc__(self) -> int:
        """B.__alloc__() -> int

        Return the number of bytes actually allocated.
        """

    def __buffer__(self, flags: int, /) -> memoryview:
        """Return a buffer object that exposes the underlying memory of the object."""

    def __release_buffer__(self, buffer: memoryview, /) -> None:
        """Release the buffer object that exposes the underlying memory of the object."""
    if sys.version_info >= (3, 14):
        def resize(self, size: int, /) -> None:
            """Resize the internal buffer of bytearray to len.

            size
              New size to resize to.
            """

_IntegerFormats: TypeAlias = Literal[
    "b", "B", "@b", "@B", "h", "H", "@h", "@H", "i", "I", "@i", "@I", "l", "L", "@l", "@L", "q", "Q", "@q", "@Q", "P", "@P"
]

@final
class memoryview(Sequence[_I]):
    """Create a new memoryview object which references the given object."""

    @property
    def format(self) -> str:
        """A string containing the format (in struct module style)
        for each element in the view.
        """

    @property
    def itemsize(self) -> int:
        """The size in bytes of each element of the memoryview."""

    @property
    def shape(self) -> tuple[int, ...] | None:
        """A tuple of ndim integers giving the shape of the memory
        as an N-dimensional array.
        """

    @property
    def strides(self) -> tuple[int, ...] | None:
        """A tuple of ndim integers giving the size in bytes to access
        each element for each dimension of the array.
        """

    @property
    def suboffsets(self) -> tuple[int, ...] | None:
        """A tuple of integers used internally for PIL-style arrays."""

    @property
    def readonly(self) -> bool:
        """A bool indicating whether the memory is read only."""

    @property
    def ndim(self) -> int:
        """An integer indicating how many dimensions of a multi-dimensional
        array the memory represents.
        """

    @property
    def obj(self) -> ReadableBuffer:
        """The underlying object of the memoryview."""

    @property
    def c_contiguous(self) -> bool:
        """A bool indicating whether the memory is C contiguous."""

    @property
    def f_contiguous(self) -> bool:
        """A bool indicating whether the memory is Fortran contiguous."""

    @property
    def contiguous(self) -> bool:
        """A bool indicating whether the memory is contiguous."""

    @property
    def nbytes(self) -> int:
        """The amount of space in bytes that the array would use in
        a contiguous representation.
        """

    def __new__(cls, obj: ReadableBuffer) -> Self: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self,
        exc_type: type[BaseException] | None,  # noqa: PYI036 # This is the module declaring BaseException
        exc_val: BaseException | None,
        exc_tb: TracebackType | None,
        /,
    ) -> None:
        """Release the underlying buffer exposed by the memoryview object."""

    @overload
    def cast(self, format: Literal["c", "@c"], shape: list[int] | tuple[int, ...] = ...) -> memoryview[bytes]:
        """Cast a memoryview to a new format or shape."""

    @overload
    def cast(self, format: Literal["f", "@f", "d", "@d"], shape: list[int] | tuple[int, ...] = ...) -> memoryview[float]: ...
    @overload
    def cast(self, format: Literal["?"], shape: list[int] | tuple[int, ...] = ...) -> memoryview[bool]: ...
    @overload
    def cast(self, format: _IntegerFormats, shape: list[int] | tuple[int, ...] = ...) -> memoryview: ...
    @overload
    def __getitem__(self, key: SupportsIndex | tuple[SupportsIndex, ...], /) -> _I:
        """Return self[key]."""

    @overload
    def __getitem__(self, key: slice, /) -> memoryview[_I]: ...
    def __contains__(self, x: object, /) -> bool: ...
    def __iter__(self) -> Iterator[_I]:
        """Implement iter(self)."""

    def __len__(self) -> int:
        """Return len(self)."""

    def __eq__(self, value: object, /) -> bool: ...
    def __hash__(self) -> int: ...
    @overload
    def __setitem__(self, key: slice, value: ReadableBuffer, /) -> None:
        """Set self[key] to value."""

    @overload
    def __setitem__(self, key: SupportsIndex | tuple[SupportsIndex, ...], value: _I, /) -> None: ...
    if sys.version_info >= (3, 10):
        def tobytes(self, order: Literal["C", "F", "A"] | None = "C") -> bytes:
            """Return the data in the buffer as a byte string.

            Order can be {'C', 'F', 'A'}. When order is 'C' or 'F', the data of the
            original array is converted to C or Fortran order. For contiguous views,
            'A' returns an exact copy of the physical memory. In particular, in-memory
            Fortran order is preserved. For non-contiguous views, the data is converted
            to C first. order=None is the same as order='C'.
            """
    else:
        def tobytes(self, order: Literal["C", "F", "A"] | None = None) -> bytes:
            """Return the data in the buffer as a byte string. Order can be {'C', 'F', 'A'}.
            When order is 'C' or 'F', the data of the original array is converted to C or
            Fortran order. For contiguous views, 'A' returns an exact copy of the physical
            memory. In particular, in-memory Fortran order is preserved. For non-contiguous
            views, the data is converted to C first. order=None is the same as order='C'.
            """

    def tolist(self) -> list[int]:
        """Return the data in the buffer as a list of elements."""

    def toreadonly(self) -> memoryview:
        """Return a readonly version of the memoryview."""

    def release(self) -> None:
        """Release the underlying buffer exposed by the memoryview object."""

    def hex(self, sep: str | bytes = ..., bytes_per_sep: SupportsIndex = 1) -> str:
        """Return the data in the buffer as a str of hexadecimal numbers.

          sep
            An optional single character or byte to separate hex bytes.
          bytes_per_sep
            How many bytes between separators.  Positive values count from the
            right, negative values count from the left.

        Example:
        >>> value = memoryview(b'\\xb9\\x01\\xef')
        >>> value.hex()
        'b901ef'
        >>> value.hex(':')
        'b9:01:ef'
        >>> value.hex(':', 2)
        'b9:01ef'
        >>> value.hex(':', -2)
        'b901:ef'
        """

    def __buffer__(self, flags: int, /) -> memoryview:
        """Return a buffer object that exposes the underlying memory of the object."""

    def __release_buffer__(self, buffer: memoryview, /) -> None:
        """Release the buffer object that exposes the underlying memory of the object."""
    if sys.version_info >= (3, 14):
        def index(self, value: object, start: SupportsIndex = 0, stop: SupportsIndex = sys.maxsize, /) -> int:
            """Return the index of the first occurrence of a value.

            Raises ValueError if the value is not present.
            """

        def count(self, value: object, /) -> int:
            """Count the number of occurrences of a value."""
    else:
        # These are inherited from the Sequence ABC, but don't actually exist on memoryview.
        # See https://github.com/python/cpython/issues/125420
        index: ClassVar[None]  # type: ignore[assignment]
        count: ClassVar[None]  # type: ignore[assignment]

    if sys.version_info >= (3, 14):
        def __class_getitem__(cls, item: Any, /) -> GenericAlias:
            """See PEP 585"""

@final
class bool(int):
    """Returns True when the argument is true, False otherwise.
    The builtins True and False are the only two instances of the class bool.
    The class bool is a subclass of the class int, and cannot be subclassed.
    """

    def __new__(cls, o: object = False, /) -> Self: ...
    # The following overloads could be represented more elegantly with a TypeVar("_B", bool, int),
    # however mypy has a bug regarding TypeVar constraints (https://github.com/python/mypy/issues/11880).
    @overload
    def __and__(self, value: bool, /) -> bool:
        """Return self&value."""

    @overload
    def __and__(self, value: int, /) -> int: ...
    @overload
    def __or__(self, value: bool, /) -> bool:
        """Return self|value."""

    @overload
    def __or__(self, value: int, /) -> int: ...
    @overload
    def __xor__(self, value: bool, /) -> bool:
        """Return self^value."""

    @overload
    def __xor__(self, value: int, /) -> int: ...
    @overload
    def __rand__(self, value: bool, /) -> bool:
        """Return value&self."""

    @overload
    def __rand__(self, value: int, /) -> int: ...
    @overload
    def __ror__(self, value: bool, /) -> bool:
        """Return value|self."""

    @overload
    def __ror__(self, value: int, /) -> int: ...
    @overload
    def __rxor__(self, value: bool, /) -> bool:
        """Return value^self."""

    @overload
    def __rxor__(self, value: int, /) -> int: ...
    def __getnewargs__(self) -> tuple[int]: ...
    @deprecated("Will throw an error in Python 3.16. Use `not` for logical negation of bools instead.")
    def __invert__(self) -> int:
        """~self"""

@final
class slice(Generic[_StartT_co, _StopT_co, _StepT_co]):
    """slice(stop)
    slice(start, stop[, step])

    Create a slice object.  This is used for extended slicing (e.g. a[0:10:2]).
    """

    @property
    def start(self) -> _StartT_co: ...
    @property
    def step(self) -> _StepT_co: ...
    @property
    def stop(self) -> _StopT_co: ...
    # Note: __new__ overloads map `None` to `Any`, since users expect slice(x, None)
    #  to be compatible with slice(None, x).
    # generic slice --------------------------------------------------------------------
    @overload
    def __new__(cls, start: None, stop: None = None, step: None = None, /) -> slice[Any, Any, Any]: ...
    # unary overloads ------------------------------------------------------------------
    @overload
    def __new__(cls, stop: _T2, /) -> slice[Any, _T2, Any]: ...
    # binary overloads -----------------------------------------------------------------
    @overload
    def __new__(cls, start: _T1, stop: None, step: None = None, /) -> slice[_T1, Any, Any]: ...
    @overload
    def __new__(cls, start: None, stop: _T2, step: None = None, /) -> slice[Any, _T2, Any]: ...
    @overload
    def __new__(cls, start: _T1, stop: _T2, step: None = None, /) -> slice[_T1, _T2, Any]: ...
    # ternary overloads ----------------------------------------------------------------
    @overload
    def __new__(cls, start: None, stop: None, step: _T3, /) -> slice[Any, Any, _T3]: ...
    @overload
    def __new__(cls, start: _T1, stop: None, step: _T3, /) -> slice[_T1, Any, _T3]: ...
    @overload
    def __new__(cls, start: None, stop: _T2, step: _T3, /) -> slice[Any, _T2, _T3]: ...
    @overload
    def __new__(cls, start: _T1, stop: _T2, step: _T3, /) -> slice[_T1, _T2, _T3]: ...
    def __eq__(self, value: object, /) -> bool: ...
    if sys.version_info >= (3, 12):
        def __hash__(self) -> int: ...
    else:
        __hash__: ClassVar[None]  # type: ignore[assignment]

    def indices(self, len: SupportsIndex, /) -> tuple[int, int, int]:
        """S.indices(len) -> (start, stop, stride)

        Assuming a sequence of length len, calculate the start and stop
        indices, and the stride length of the extended slice described by
        S. Out of bounds indices are clipped in a manner consistent with the
        handling of normal slices.
        """

@disjoint_base
class tuple(Sequence[_T_co]):
    """Built-in immutable sequence.

    If no argument is given, the constructor returns an empty tuple.
    If iterable is specified the tuple is initialized from iterable's items.

    If the argument is a tuple, the return value is the same object.
    """

    def __new__(cls, iterable: Iterable[_T_co] = (), /) -> Self: ...
    def __len__(self) -> int:
        """Return len(self)."""

    def __contains__(self, key: object, /) -> bool:
        """Return bool(key in self)."""

    @overload
    def __getitem__(self, key: SupportsIndex, /) -> _T_co:
        """Return self[key]."""

    @overload
    def __getitem__(self, key: slice, /) -> tuple[_T_co, ...]: ...
    def __iter__(self) -> Iterator[_T_co]:
        """Implement iter(self)."""

    def __lt__(self, value: tuple[_T_co, ...], /) -> bool: ...
    def __le__(self, value: tuple[_T_co, ...], /) -> bool: ...
    def __gt__(self, value: tuple[_T_co, ...], /) -> bool: ...
    def __ge__(self, value: tuple[_T_co, ...], /) -> bool: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __hash__(self) -> int: ...
    @overload
    def __add__(self, value: tuple[_T_co, ...], /) -> tuple[_T_co, ...]:
        """Return self+value."""

    @overload
    def __add__(self, value: tuple[_T, ...], /) -> tuple[_T_co | _T, ...]: ...
    def __mul__(self, value: SupportsIndex, /) -> tuple[_T_co, ...]:
        """Return self*value."""

    def __rmul__(self, value: SupportsIndex, /) -> tuple[_T_co, ...]:
        """Return value*self."""

    def count(self, value: Any, /) -> int:
        """Return number of occurrences of value."""

    def index(self, value: Any, start: SupportsIndex = 0, stop: SupportsIndex = sys.maxsize, /) -> int:
        """Return first index of value.

        Raises ValueError if the value is not present.
        """

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

# Doesn't exist at runtime, but deleting this breaks mypy and pyright. See:
# https://github.com/python/typeshed/issues/7580
# https://github.com/python/mypy/issues/8240
# Obsolete, use types.FunctionType instead.
@final
@type_check_only
class function:
    # Make sure this class definition stays roughly in line with `types.FunctionType`
    @property
    def __closure__(self) -> tuple[CellType, ...] | None: ...
    __code__: CodeType
    __defaults__: tuple[Any, ...] | None
    __dict__: dict[str, Any]
    @property
    def __globals__(self) -> dict[str, Any]: ...
    __name__: str
    __qualname__: str
    __annotations__: dict[str, AnnotationForm]
    if sys.version_info >= (3, 14):
        __annotate__: AnnotateFunc | None
    __kwdefaults__: dict[str, Any] | None
    if sys.version_info >= (3, 10):
        @property
        def __builtins__(self) -> dict[str, Any]: ...
    if sys.version_info >= (3, 12):
        __type_params__: tuple[TypeVar | ParamSpec | TypeVarTuple, ...]

    __module__: str
    if sys.version_info >= (3, 13):
        def __new__(
            cls,
            code: CodeType,
            globals: dict[str, Any],
            name: str | None = None,
            argdefs: tuple[object, ...] | None = None,
            closure: tuple[CellType, ...] | None = None,
            kwdefaults: dict[str, object] | None = None,
        ) -> Self: ...
    else:
        def __new__(
            cls,
            code: CodeType,
            globals: dict[str, Any],
            name: str | None = None,
            argdefs: tuple[object, ...] | None = None,
            closure: tuple[CellType, ...] | None = None,
        ) -> Self: ...

    # mypy uses `builtins.function.__get__` to represent methods, properties, and getset_descriptors so we type the return as Any.
    def __get__(self, instance: object, owner: type | None = None, /) -> Any: ...

@disjoint_base
class list(MutableSequence[_T]):
    """Built-in mutable sequence.

    If no argument is given, the constructor creates a new empty list.
    The argument must be an iterable if specified.
    """

    @overload
    def __init__(self) -> None: ...
    @overload
    def __init__(self, iterable: Iterable[_T], /) -> None: ...
    def copy(self) -> list[_T]:
        """Return a shallow copy of the list."""

    def append(self, object: _T, /) -> None:
        """Append object to the end of the list."""

    def extend(self, iterable: Iterable[_T], /) -> None:
        """Extend list by appending elements from the iterable."""

    def pop(self, index: SupportsIndex = -1, /) -> _T:
        """Remove and return item at index (default last).

        Raises IndexError if list is empty or index is out of range.
        """
    # Signature of `list.index` should be kept in line with `collections.UserList.index()`
    # and multiprocessing.managers.ListProxy.index()
    def index(self, value: _T, start: SupportsIndex = 0, stop: SupportsIndex = sys.maxsize, /) -> int:
        """Return first index of value.

        Raises ValueError if the value is not present.
        """

    def count(self, value: _T, /) -> int:
        """Return number of occurrences of value."""

    def insert(self, index: SupportsIndex, object: _T, /) -> None:
        """Insert object before index."""

    def remove(self, value: _T, /) -> None:
        """Remove first occurrence of value.

        Raises ValueError if the value is not present.
        """
    # Signature of `list.sort` should be kept inline with `collections.UserList.sort()`
    # and multiprocessing.managers.ListProxy.sort()
    #
    # Use list[SupportsRichComparisonT] for the first overload rather than [SupportsRichComparison]
    # to work around invariance
    @overload
    def sort(self: list[SupportsRichComparisonT], *, key: None = None, reverse: bool = False) -> None:
        """Sort the list in ascending order and return None.

        The sort is in-place (i.e. the list itself is modified) and stable (i.e. the
        order of two equal elements is maintained).

        If a key function is given, apply it once to each list item and sort them,
        ascending or descending, according to their function values.

        The reverse flag can be set to sort in descending order.
        """

    @overload
    def sort(self, *, key: Callable[[_T], SupportsRichComparison], reverse: bool = False) -> None: ...
    def __len__(self) -> int:
        """Return len(self)."""

    def __iter__(self) -> Iterator[_T]:
        """Implement iter(self)."""
    __hash__: ClassVar[None]  # type: ignore[assignment]
    @overload
    def __getitem__(self, i: SupportsIndex, /) -> _T:
        """Return self[index]."""

    @overload
    def __getitem__(self, s: slice, /) -> list[_T]: ...
    @overload
    def __setitem__(self, key: SupportsIndex, value: _T, /) -> None:
        """Set self[key] to value."""

    @overload
    def __setitem__(self, key: slice, value: Iterable[_T], /) -> None: ...
    def __delitem__(self, key: SupportsIndex | slice, /) -> None:
        """Delete self[key]."""
    # Overloading looks unnecessary, but is needed to work around complex mypy problems
    @overload
    def __add__(self, value: list[_T], /) -> list[_T]:
        """Return self+value."""

    @overload
    def __add__(self, value: list[_S], /) -> list[_S | _T]: ...
    def __iadd__(self, value: Iterable[_T], /) -> Self:  # type: ignore[misc]
        """Implement self+=value."""

    def __mul__(self, value: SupportsIndex, /) -> list[_T]:
        """Return self*value."""

    def __rmul__(self, value: SupportsIndex, /) -> list[_T]:
        """Return value*self."""

    def __imul__(self, value: SupportsIndex, /) -> Self:
        """Implement self*=value."""

    def __contains__(self, key: object, /) -> bool:
        """Return bool(key in self)."""

    def __reversed__(self) -> Iterator[_T]:
        """Return a reverse iterator over the list."""

    def __gt__(self, value: list[_T], /) -> bool: ...
    def __ge__(self, value: list[_T], /) -> bool: ...
    def __lt__(self, value: list[_T], /) -> bool: ...
    def __le__(self, value: list[_T], /) -> bool: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

@disjoint_base
class dict(MutableMapping[_KT, _VT]):
    """dict() -> new empty dictionary
    dict(mapping) -> new dictionary initialized from a mapping object's
        (key, value) pairs
    dict(iterable) -> new dictionary initialized as if via:
        d = {}
        for k, v in iterable:
            d[k] = v
    dict(**kwargs) -> new dictionary initialized with the name=value pairs
        in the keyword argument list.  For example:  dict(one=1, two=2)
    """

    # __init__ should be kept roughly in line with `collections.UserDict.__init__`, which has similar semantics
    # Also multiprocessing.managers.SyncManager.dict()
    @overload
    def __init__(self) -> None: ...
    @overload
    def __init__(self: dict[str, _VT], **kwargs: _VT) -> None: ...  # pyright: ignore[reportInvalidTypeVarUse]  #11780
    @overload
    def __init__(self, map: SupportsKeysAndGetItem[_KT, _VT], /) -> None: ...
    @overload
    def __init__(
        self: dict[str, _VT],  # pyright: ignore[reportInvalidTypeVarUse]  #11780
        map: SupportsKeysAndGetItem[str, _VT],
        /,
        **kwargs: _VT,
    ) -> None: ...
    @overload
    def __init__(self, iterable: Iterable[tuple[_KT, _VT]], /) -> None: ...
    @overload
    def __init__(
        self: dict[str, _VT],  # pyright: ignore[reportInvalidTypeVarUse]  #11780
        iterable: Iterable[tuple[str, _VT]],
        /,
        **kwargs: _VT,
    ) -> None: ...
    # Next two overloads are for dict(string.split(sep) for string in iterable)
    # Cannot be Iterable[Sequence[_T]] or otherwise dict(["foo", "bar", "baz"]) is not an error
    @overload
    def __init__(self: dict[str, str], iterable: Iterable[list[str]], /) -> None: ...
    @overload
    def __init__(self: dict[bytes, bytes], iterable: Iterable[list[bytes]], /) -> None: ...
    def __new__(cls, *args: Any, **kwargs: Any) -> Self: ...
    def copy(self) -> dict[_KT, _VT]:
        """Return a shallow copy of the dict."""

    def keys(self) -> dict_keys[_KT, _VT]:
        """Return a set-like object providing a view on the dict's keys."""

    def values(self) -> dict_values[_KT, _VT]:
        """Return an object providing a view on the dict's values."""

    def items(self) -> dict_items[_KT, _VT]:
        """Return a set-like object providing a view on the dict's items."""
    # Signature of `dict.fromkeys` should be kept identical to
    # `fromkeys` methods of `OrderedDict`/`ChainMap`/`UserDict` in `collections`
    # TODO: the true signature of `dict.fromkeys` is not expressible in the current type system.
    # See #3800 & https://github.com/python/typing/issues/548#issuecomment-683336963.
    @classmethod
    @overload
    def fromkeys(cls, iterable: Iterable[_T], value: None = None, /) -> dict[_T, Any | None]:
        """Create a new dictionary with keys from iterable and values set to value."""

    @classmethod
    @overload
    def fromkeys(cls, iterable: Iterable[_T], value: _S, /) -> dict[_T, _S]: ...
    # Positional-only in dict, but not in MutableMapping
    @overload  # type: ignore[override]
    def get(self, key: _KT, default: None = None, /) -> _VT | None:
        """Return the value for key if key is in the dictionary, else default."""

    @overload
    def get(self, key: _KT, default: _VT, /) -> _VT: ...
    @overload
    def get(self, key: _KT, default: _T, /) -> _VT | _T: ...
    @overload
    def pop(self, key: _KT, /) -> _VT:
        """D.pop(k[,d]) -> v, remove specified key and return the corresponding value.

        If the key is not found, return the default if given; otherwise,
        raise a KeyError.
        """

    @overload
    def pop(self, key: _KT, default: _VT, /) -> _VT: ...
    @overload
    def pop(self, key: _KT, default: _T, /) -> _VT | _T: ...
    def __len__(self) -> int:
        """Return len(self)."""

    def __getitem__(self, key: _KT, /) -> _VT:
        """Return self[key]."""

    def __setitem__(self, key: _KT, value: _VT, /) -> None:
        """Set self[key] to value."""

    def __delitem__(self, key: _KT, /) -> None:
        """Delete self[key]."""

    def __iter__(self) -> Iterator[_KT]:
        """Implement iter(self)."""

    def __eq__(self, value: object, /) -> bool: ...
    def __reversed__(self) -> Iterator[_KT]:
        """Return a reverse iterator over the dict keys."""
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

    @overload
    def __or__(self, value: dict[_KT, _VT], /) -> dict[_KT, _VT]:
        """Return self|value."""

    @overload
    def __or__(self, value: dict[_T1, _T2], /) -> dict[_KT | _T1, _VT | _T2]: ...
    @overload
    def __ror__(self, value: dict[_KT, _VT], /) -> dict[_KT, _VT]:
        """Return value|self."""

    @overload
    def __ror__(self, value: dict[_T1, _T2], /) -> dict[_KT | _T1, _VT | _T2]: ...
    # dict.__ior__ should be kept roughly in line with MutableMapping.update()
    @overload  # type: ignore[misc]
    def __ior__(self, value: SupportsKeysAndGetItem[_KT, _VT], /) -> Self:
        """Return self|=value."""

    @overload
    def __ior__(self, value: Iterable[tuple[_KT, _VT]], /) -> Self: ...

@disjoint_base
class set(MutableSet[_T]):
    """Build an unordered collection of unique elements."""

    @overload
    def __init__(self) -> None: ...
    @overload
    def __init__(self, iterable: Iterable[_T], /) -> None: ...
    def add(self, element: _T, /) -> None:
        """Add an element to a set.

        This has no effect if the element is already present.
        """

    def copy(self) -> set[_T]:
        """Return a shallow copy of a set."""

    def difference(self, *s: Iterable[Any]) -> set[_T]:
        """Return a new set with elements in the set that are not in the others."""

    def difference_update(self, *s: Iterable[Any]) -> None:
        """Update the set, removing elements found in others."""

    def discard(self, element: _T, /) -> None:
        """Remove an element from a set if it is a member.

        Unlike set.remove(), the discard() method does not raise
        an exception when an element is missing from the set.
        """

    def intersection(self, *s: Iterable[Any]) -> set[_T]:
        """Return a new set with elements common to the set and all others."""

    def intersection_update(self, *s: Iterable[Any]) -> None:
        """Update the set, keeping only elements found in it and all others."""

    def isdisjoint(self, s: Iterable[Any], /) -> bool:
        """Return True if two sets have a null intersection."""

    def issubset(self, s: Iterable[Any], /) -> bool:
        """Report whether another set contains this set."""

    def issuperset(self, s: Iterable[Any], /) -> bool:
        """Report whether this set contains another set."""

    def remove(self, element: _T, /) -> None:
        """Remove an element from a set; it must be a member.

        If the element is not a member, raise a KeyError.
        """

    def symmetric_difference(self, s: Iterable[_T], /) -> set[_T]:
        """Return a new set with elements in either the set or other but not both."""

    def symmetric_difference_update(self, s: Iterable[_T], /) -> None:
        """Update the set, keeping only elements found in either set, but not in both."""

    def union(self, *s: Iterable[_S]) -> set[_T | _S]:
        """Return a new set with elements from the set and all others."""

    def update(self, *s: Iterable[_T]) -> None:
        """Update the set, adding elements from all others."""

    def __len__(self) -> int:
        """Return len(self)."""

    def __contains__(self, o: object, /) -> bool:
        """x.__contains__(y) <==> y in x."""

    def __iter__(self) -> Iterator[_T]:
        """Implement iter(self)."""

    def __and__(self, value: AbstractSet[object], /) -> set[_T]:
        """Return self&value."""

    def __iand__(self, value: AbstractSet[object], /) -> Self:
        """Return self&=value."""

    def __or__(self, value: AbstractSet[_S], /) -> set[_T | _S]:
        """Return self|value."""

    def __ior__(self, value: AbstractSet[_T], /) -> Self:  # type: ignore[override,misc]
        """Return self|=value."""

    def __sub__(self, value: AbstractSet[_T | None], /) -> set[_T]:
        """Return self-value."""

    def __isub__(self, value: AbstractSet[object], /) -> Self:
        """Return self-=value."""

    def __xor__(self, value: AbstractSet[_S], /) -> set[_T | _S]:
        """Return self^value."""

    def __ixor__(self, value: AbstractSet[_T], /) -> Self:  # type: ignore[override,misc]
        """Return self^=value."""

    def __le__(self, value: AbstractSet[object], /) -> bool: ...
    def __lt__(self, value: AbstractSet[object], /) -> bool: ...
    def __ge__(self, value: AbstractSet[object], /) -> bool: ...
    def __gt__(self, value: AbstractSet[object], /) -> bool: ...
    def __eq__(self, value: object, /) -> bool: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

@disjoint_base
class frozenset(AbstractSet[_T_co]):
    """Build an immutable unordered collection of unique elements."""

    @overload
    def __new__(cls) -> Self: ...
    @overload
    def __new__(cls, iterable: Iterable[_T_co], /) -> Self: ...
    def copy(self) -> frozenset[_T_co]:
        """Return a shallow copy of a set."""

    def difference(self, *s: Iterable[object]) -> frozenset[_T_co]:
        """Return a new set with elements in the set that are not in the others."""

    def intersection(self, *s: Iterable[object]) -> frozenset[_T_co]:
        """Return a new set with elements common to the set and all others."""

    def isdisjoint(self, s: Iterable[_T_co], /) -> bool:
        """Return True if two sets have a null intersection."""

    def issubset(self, s: Iterable[object], /) -> bool:
        """Report whether another set contains this set."""

    def issuperset(self, s: Iterable[object], /) -> bool:
        """Report whether this set contains another set."""

    def symmetric_difference(self, s: Iterable[_T_co], /) -> frozenset[_T_co]:
        """Return a new set with elements in either the set or other but not both."""

    def union(self, *s: Iterable[_S]) -> frozenset[_T_co | _S]:
        """Return a new set with elements from the set and all others."""

    def __len__(self) -> int:
        """Return len(self)."""

    def __contains__(self, o: object, /) -> bool:
        """x.__contains__(y) <==> y in x."""

    def __iter__(self) -> Iterator[_T_co]:
        """Implement iter(self)."""

    def __and__(self, value: AbstractSet[_T_co], /) -> frozenset[_T_co]:
        """Return self&value."""

    def __or__(self, value: AbstractSet[_S], /) -> frozenset[_T_co | _S]:
        """Return self|value."""

    def __sub__(self, value: AbstractSet[_T_co], /) -> frozenset[_T_co]:
        """Return self-value."""

    def __xor__(self, value: AbstractSet[_S], /) -> frozenset[_T_co | _S]:
        """Return self^value."""

    def __le__(self, value: AbstractSet[object], /) -> bool: ...
    def __lt__(self, value: AbstractSet[object], /) -> bool: ...
    def __ge__(self, value: AbstractSet[object], /) -> bool: ...
    def __gt__(self, value: AbstractSet[object], /) -> bool: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __hash__(self) -> int: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

@disjoint_base
class enumerate(Generic[_T]):
    """Return an enumerate object.

      iterable
        an object supporting iteration

    The enumerate object yields pairs containing a count (from start, which
    defaults to zero) and a value yielded by the iterable argument.

    enumerate is useful for obtaining an indexed list:
        (0, seq[0]), (1, seq[1]), (2, seq[2]), ...
    """

    def __new__(cls, iterable: Iterable[_T], start: int = 0) -> Self: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> tuple[int, _T]:
        """Implement next(self)."""

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

@final
class range(Sequence[int]):
    """range(stop) -> range object
    range(start, stop[, step]) -> range object

    Return an object that produces a sequence of integers from start (inclusive)
    to stop (exclusive) by step.  range(i, j) produces i, i+1, i+2, ..., j-1.
    start defaults to 0, and stop is omitted!  range(4) produces 0, 1, 2, 3.
    These are exactly the valid indices for a list of 4 elements.
    When step is given, it specifies the increment (or decrement).
    """

    @property
    def start(self) -> int: ...
    @property
    def stop(self) -> int: ...
    @property
    def step(self) -> int: ...
    @overload
    def __new__(cls, stop: SupportsIndex, /) -> Self: ...
    @overload
    def __new__(cls, start: SupportsIndex, stop: SupportsIndex, step: SupportsIndex = 1, /) -> Self: ...
    def count(self, value: int, /) -> int:
        """rangeobject.count(value) -> integer -- return number of occurrences of value"""

    def index(self, value: int, /) -> int:  # type: ignore[override]
        """rangeobject.index(value) -> integer -- return index of value.
        Raise ValueError if the value is not present.
        """

    def __len__(self) -> int:
        """Return len(self)."""

    def __eq__(self, value: object, /) -> bool: ...
    def __hash__(self) -> int: ...
    def __contains__(self, key: object, /) -> bool:
        """Return bool(key in self)."""

    def __iter__(self) -> Iterator[int]:
        """Implement iter(self)."""

    @overload
    def __getitem__(self, key: SupportsIndex, /) -> int:
        """Return self[key]."""

    @overload
    def __getitem__(self, key: slice, /) -> range: ...
    def __reversed__(self) -> Iterator[int]:
        """Return a reverse iterator."""

@disjoint_base
class property:
    """Property attribute.

      fget
        function to be used for getting an attribute value
      fset
        function to be used for setting an attribute value
      fdel
        function to be used for del'ing an attribute
      doc
        docstring

    Typical use is to define a managed attribute x:

    class C(object):
        def getx(self): return self._x
        def setx(self, value): self._x = value
        def delx(self): del self._x
        x = property(getx, setx, delx, "I'm the 'x' property.")

    Decorators make defining new properties or modifying existing ones easy:

    class C(object):
        @property
        def x(self):
            "I am the 'x' property."
            return self._x
        @x.setter
        def x(self, value):
            self._x = value
        @x.deleter
        def x(self):
            del self._x
    """

    fget: Callable[[Any], Any] | None
    fset: Callable[[Any, Any], None] | None
    fdel: Callable[[Any], None] | None
    __isabstractmethod__: bool
    if sys.version_info >= (3, 13):
        __name__: str

    def __init__(
        self,
        fget: Callable[[Any], Any] | None = None,
        fset: Callable[[Any, Any], None] | None = None,
        fdel: Callable[[Any], None] | None = None,
        doc: str | None = None,
    ) -> None: ...
    def getter(self, fget: Callable[[Any], Any], /) -> property:
        """Descriptor to obtain a copy of the property with a different getter."""

    def setter(self, fset: Callable[[Any, Any], None], /) -> property:
        """Descriptor to obtain a copy of the property with a different setter."""

    def deleter(self, fdel: Callable[[Any], None], /) -> property:
        """Descriptor to obtain a copy of the property with a different deleter."""

    @overload
    def __get__(self, instance: None, owner: type, /) -> Self:
        """Return an attribute of instance, which is of type owner."""

    @overload
    def __get__(self, instance: Any, owner: type | None = None, /) -> Any: ...
    def __set__(self, instance: Any, value: Any, /) -> None:
        """Set an attribute of instance to value."""

    def __delete__(self, instance: Any, /) -> None:
        """Delete an attribute of instance."""

def abs(x: SupportsAbs[_T], /) -> _T:
    """Return the absolute value of the argument."""

def all(iterable: Iterable[object], /) -> bool:
    """Return True if bool(x) is True for all values x in the iterable.

    If the iterable is empty, return True.
    """

def any(iterable: Iterable[object], /) -> bool:
    """Return True if bool(x) is True for any x in the iterable.

    If the iterable is empty, return False.
    """

def ascii(obj: object, /) -> str:
    """Return an ASCII-only representation of an object.

    As repr(), return a string containing a printable representation of an
    object, but escape the non-ASCII characters in the string returned by
    repr() using \\\\x, \\\\u or \\\\U escapes. This generates a string similar
    to that returned by repr() in Python 2.
    """

def bin(number: int | SupportsIndex, /) -> str:
    """Return the binary representation of an integer.

    >>> bin(2796202)
    '0b1010101010101010101010'
    """

def breakpoint(*args: Any, **kws: Any) -> None:
    """Call sys.breakpointhook(*args, **kws).  sys.breakpointhook() must accept
    whatever arguments are passed.

    By default, this drops you into the pdb debugger.
    """

def callable(obj: object, /) -> TypeIs[Callable[..., object]]:
    """Return whether the object is callable (i.e., some kind of function).

    Note that classes are callable, as are instances of classes with a
    __call__() method.
    """

def chr(i: int | SupportsIndex, /) -> str:
    """Return a Unicode string of one character with ordinal i; 0 <= i <= 0x10ffff."""

if sys.version_info >= (3, 10):
    def aiter(async_iterable: SupportsAiter[_SupportsAnextT_co], /) -> _SupportsAnextT_co:
        """Return an AsyncIterator for an AsyncIterable object."""

    @type_check_only
    class _SupportsSynchronousAnext(Protocol[_AwaitableT_co]):
        def __anext__(self) -> _AwaitableT_co: ...

    @overload
    # `anext` is not, in fact, an async function. When default is not provided
    # `anext` is just a passthrough for `obj.__anext__`
    # See discussion in #7491 and pure-Python implementation of `anext` at https://github.com/python/cpython/blob/ea786a882b9ed4261eafabad6011bc7ef3b5bf94/Lib/test/test_asyncgen.py#L52-L80
    def anext(i: _SupportsSynchronousAnext[_AwaitableT], /) -> _AwaitableT:
        """Return the next item from the async iterator.

        If default is given and the async iterator is exhausted,
        it is returned instead of raising StopAsyncIteration.
        """

    @overload
    async def anext(i: SupportsAnext[_T], default: _VT, /) -> _T | _VT: ...

# compile() returns a CodeType, unless the flags argument includes PyCF_ONLY_AST (=1024),
# in which case it returns ast.AST. We have overloads for flag 0 (the default) and for
# explicitly passing PyCF_ONLY_AST. We fall back to Any for other values of flags.
@overload
def compile(
    source: str | ReadableBuffer | _ast.Module | _ast.Expression | _ast.Interactive,
    filename: str | bytes | PathLike[Any],
    mode: str,
    flags: Literal[0],
    dont_inherit: bool = False,
    optimize: int = -1,
    *,
    _feature_version: int = -1,
) -> CodeType:
    """Compile source into a code object that can be executed by exec() or eval().

    The source code may represent a Python module, statement or expression.
    The filename will be used for run-time error messages.
    The mode must be 'exec' to compile a module, 'single' to compile a
    single (interactive) statement, or 'eval' to compile an expression.
    The flags argument, if present, controls which future statements influence
    the compilation of the code.
    The dont_inherit argument, if true, stops the compilation inheriting
    the effects of any future statements in effect in the code calling
    compile; if absent or false these statements do influence the compilation,
    in addition to any features explicitly specified.
    """

@overload
def compile(
    source: str | ReadableBuffer | _ast.Module | _ast.Expression | _ast.Interactive,
    filename: str | bytes | PathLike[Any],
    mode: str,
    *,
    dont_inherit: bool = False,
    optimize: int = -1,
    _feature_version: int = -1,
) -> CodeType: ...
@overload
def compile(
    source: str | ReadableBuffer | _ast.Module | _ast.Expression | _ast.Interactive,
    filename: str | bytes | PathLike[Any],
    mode: str,
    flags: Literal[1024],
    dont_inherit: bool = False,
    optimize: int = -1,
    *,
    _feature_version: int = -1,
) -> _ast.AST: ...
@overload
def compile(
    source: str | ReadableBuffer | _ast.Module | _ast.Expression | _ast.Interactive,
    filename: str | bytes | PathLike[Any],
    mode: str,
    flags: int,
    dont_inherit: bool = False,
    optimize: int = -1,
    *,
    _feature_version: int = -1,
) -> Any: ...

copyright: _sitebuiltins._Printer
credits: _sitebuiltins._Printer

def delattr(obj: object, name: str, /) -> None:
    """Deletes the named attribute from the given object.

    delattr(x, 'y') is equivalent to ``del x.y``
    """

def dir(o: object = ..., /) -> list[str]:
    """dir([object]) -> list of strings

    If called without an argument, return the names in the current scope.
    Else, return an alphabetized list of names comprising (some of) the attributes
    of the given object, and of attributes reachable from it.
    If the object supplies a method named __dir__, it will be used; otherwise
    the default dir() logic is used and returns:
      for a module object: the module's attributes.
      for a class object:  its attributes, and recursively the attributes
        of its bases.
      for any other object: its attributes, its class's attributes, and
        recursively the attributes of its class's base classes.
    """

@overload
def divmod(x: SupportsDivMod[_T_contra, _T_co], y: _T_contra, /) -> _T_co:
    """Return the tuple (x//y, x%y).  Invariant: div*y + mod == x."""

@overload
def divmod(x: _T_contra, y: SupportsRDivMod[_T_contra, _T_co], /) -> _T_co: ...

# The `globals` argument to `eval` has to be `dict[str, Any]` rather than `dict[str, object]` due to invariance.
# (The `globals` argument has to be a "real dict", rather than any old mapping, unlike the `locals` argument.)
if sys.version_info >= (3, 13):
    def eval(
        source: str | ReadableBuffer | CodeType,
        /,
        globals: dict[str, Any] | None = None,
        locals: Mapping[str, object] | None = None,
    ) -> Any:
        """Evaluate the given source in the context of globals and locals.

        The source may be a string representing a Python expression
        or a code object as returned by compile().
        The globals must be a dictionary and locals can be any mapping,
        defaulting to the current globals and locals.
        If only globals is given, locals defaults to it.
        """

else:
    def eval(
        source: str | ReadableBuffer | CodeType,
        globals: dict[str, Any] | None = None,
        locals: Mapping[str, object] | None = None,
        /,
    ) -> Any:
        """Evaluate the given source in the context of globals and locals.

        The source may be a string representing a Python expression
        or a code object as returned by compile().
        The globals must be a dictionary and locals can be any mapping,
        defaulting to the current globals and locals.
        If only globals is given, locals defaults to it.
        """

# Comment above regarding `eval` applies to `exec` as well
if sys.version_info >= (3, 13):
    def exec(
        source: str | ReadableBuffer | CodeType,
        /,
        globals: dict[str, Any] | None = None,
        locals: Mapping[str, object] | None = None,
        *,
        closure: tuple[CellType, ...] | None = None,
    ) -> None:
        """Execute the given source in the context of globals and locals.

        The source may be a string representing one or more Python statements
        or a code object as returned by compile().
        The globals must be a dictionary and locals can be any mapping,
        defaulting to the current globals and locals.
        If only globals is given, locals defaults to it.
        The closure must be a tuple of cellvars, and can only be used
        when source is a code object requiring exactly that many cellvars.
        """

elif sys.version_info >= (3, 11):
    def exec(
        source: str | ReadableBuffer | CodeType,
        globals: dict[str, Any] | None = None,
        locals: Mapping[str, object] | None = None,
        /,
        *,
        closure: tuple[CellType, ...] | None = None,
    ) -> None:
        """Execute the given source in the context of globals and locals.

        The source may be a string representing one or more Python statements
        or a code object as returned by compile().
        The globals must be a dictionary and locals can be any mapping,
        defaulting to the current globals and locals.
        If only globals is given, locals defaults to it.
        The closure must be a tuple of cellvars, and can only be used
        when source is a code object requiring exactly that many cellvars.
        """

else:
    def exec(
        source: str | ReadableBuffer | CodeType,
        globals: dict[str, Any] | None = None,
        locals: Mapping[str, object] | None = None,
        /,
    ) -> None:
        """Execute the given source in the context of globals and locals.

        The source may be a string representing one or more Python statements
        or a code object as returned by compile().
        The globals must be a dictionary and locals can be any mapping,
        defaulting to the current globals and locals.
        If only globals is given, locals defaults to it.
        """

exit: _sitebuiltins.Quitter

@disjoint_base
class filter(Generic[_T]):
    """Return an iterator yielding those items of iterable for which function(item)
    is true. If function is None, return the items that are true.
    """

    @overload
    def __new__(cls, function: None, iterable: Iterable[_T | None], /) -> Self: ...
    @overload
    def __new__(cls, function: Callable[[_S], TypeGuard[_T]], iterable: Iterable[_S], /) -> Self: ...
    @overload
    def __new__(cls, function: Callable[[_S], TypeIs[_T]], iterable: Iterable[_S], /) -> Self: ...
    @overload
    def __new__(cls, function: Callable[[_T], Any], iterable: Iterable[_T], /) -> Self: ...
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T:
        """Implement next(self)."""

def format(value: object, format_spec: str = "", /) -> str:
    """Return type(value).__format__(value, format_spec)

    Many built-in types implement format_spec according to the
    Format Specification Mini-language. See help('FORMATTING').

    If type(value) does not supply a method named __format__
    and format_spec is empty, then str(value) is returned.
    See also help('SPECIALMETHODS').
    """

@overload
def getattr(o: object, name: str, /) -> Any:
    """getattr(object, name[, default]) -> value

    Get a named attribute from an object; getattr(x, 'y') is equivalent to x.y.
    When a default argument is given, it is returned when the attribute doesn't
    exist; without it, an exception is raised in that case.
    """

# While technically covered by the last overload, spelling out the types for None, bool
# and basic containers help mypy out in some tricky situations involving type context
# (aka bidirectional inference)
@overload
def getattr(o: object, name: str, default: None, /) -> Any | None: ...
@overload
def getattr(o: object, name: str, default: bool, /) -> Any | bool: ...
@overload
def getattr(o: object, name: str, default: list[Any], /) -> Any | list[Any]: ...
@overload
def getattr(o: object, name: str, default: dict[Any, Any], /) -> Any | dict[Any, Any]: ...
@overload
def getattr(o: object, name: str, default: _T, /) -> Any | _T: ...
def globals() -> dict[str, Any]:
    """Return the dictionary containing the current scope's global variables.

    NOTE: Updates to this dictionary *will* affect name lookups in the current
    global scope and vice-versa.
    """

def hasattr(obj: object, name: str, /) -> bool:
    """Return whether the object has an attribute with the given name.

    This is done by calling getattr(obj, name) and catching AttributeError.
    """

def hash(obj: object, /) -> int:
    """Return the hash value for the given object.

    Two objects that compare equal must also have the same hash value, but the
    reverse is not necessarily true.
    """

help: _sitebuiltins._Helper

def hex(number: int | SupportsIndex, /) -> str:
    """Return the hexadecimal representation of an integer.

    >>> hex(12648430)
    '0xc0ffee'
    """

def id(obj: object, /) -> int:
    """Return the identity of an object.

    This is guaranteed to be unique among simultaneously existing objects.
    (CPython uses the object's memory address.)
    """

def input(prompt: object = "", /) -> str:
    """Read a string from standard input.  The trailing newline is stripped.

    The prompt string, if given, is printed to standard output without a
    trailing newline before reading input.

    If the user hits EOF (*nix: Ctrl-D, Windows: Ctrl-Z+Return), raise EOFError.
    On *nix systems, readline is used if available.
    """

@type_check_only
class _GetItemIterable(Protocol[_T_co]):
    def __getitem__(self, i: int, /) -> _T_co: ...

@overload
def iter(object: SupportsIter[_SupportsNextT_co], /) -> _SupportsNextT_co:
    """iter(iterable) -> iterator
    iter(callable, sentinel) -> iterator

    Get an iterator from an object.  In the first form, the argument must
    supply its own iterator, or be a sequence.
    In the second form, the callable is called until it returns the sentinel.
    """

@overload
def iter(object: _GetItemIterable[_T], /) -> Iterator[_T]: ...
@overload
def iter(object: Callable[[], _T | None], sentinel: None, /) -> Iterator[_T]: ...
@overload
def iter(object: Callable[[], _T], sentinel: object, /) -> Iterator[_T]: ...

if sys.version_info >= (3, 10):
    _ClassInfo: TypeAlias = type | types.UnionType | tuple[_ClassInfo, ...]
else:
    _ClassInfo: TypeAlias = type | tuple[_ClassInfo, ...]

def isinstance(obj: object, class_or_tuple: _ClassInfo, /) -> bool:
    """Return whether an object is an instance of a class or of a subclass thereof.

    A tuple, as in ``isinstance(x, (A, B, ...))``, may be given as the target to
    check against. This is equivalent to ``isinstance(x, A) or isinstance(x, B)
    or ...`` etc.
    """

def issubclass(cls: type, class_or_tuple: _ClassInfo, /) -> bool:
    """Return whether 'cls' is derived from another class or is the same class.

    A tuple, as in ``issubclass(x, (A, B, ...))``, may be given as the target to
    check against. This is equivalent to ``issubclass(x, A) or issubclass(x, B)
    or ...``.
    """

def len(obj: Sized, /) -> int:
    """Return the number of items in a container."""

license: _sitebuiltins._Printer

def locals() -> dict[str, Any]:
    """Return a dictionary containing the current scope's local variables.

    NOTE: Whether or not updates to this dictionary will affect name lookups in
    the local scope and vice-versa is *implementation dependent* and not
    covered by any backwards compatibility guarantees.
    """

@disjoint_base
class map(Generic[_S]):
    """Make an iterator that computes the function using arguments from
    each of the iterables.  Stops when the shortest iterable is exhausted.

    If strict is true and one of the arguments is exhausted before the others,
    raise a ValueError.
    """

    # 3.14 adds `strict` argument.
    if sys.version_info >= (3, 14):
        @overload
        def __new__(cls, func: Callable[[_T1], _S], iterable: Iterable[_T1], /, *, strict: bool = False) -> Self: ...
        @overload
        def __new__(
            cls, func: Callable[[_T1, _T2], _S], iterable: Iterable[_T1], iter2: Iterable[_T2], /, *, strict: bool = False
        ) -> Self: ...
        @overload
        def __new__(
            cls,
            func: Callable[[_T1, _T2, _T3], _S],
            iterable: Iterable[_T1],
            iter2: Iterable[_T2],
            iter3: Iterable[_T3],
            /,
            *,
            strict: bool = False,
        ) -> Self: ...
        @overload
        def __new__(
            cls,
            func: Callable[[_T1, _T2, _T3, _T4], _S],
            iterable: Iterable[_T1],
            iter2: Iterable[_T2],
            iter3: Iterable[_T3],
            iter4: Iterable[_T4],
            /,
            *,
            strict: bool = False,
        ) -> Self: ...
        @overload
        def __new__(
            cls,
            func: Callable[[_T1, _T2, _T3, _T4, _T5], _S],
            iterable: Iterable[_T1],
            iter2: Iterable[_T2],
            iter3: Iterable[_T3],
            iter4: Iterable[_T4],
            iter5: Iterable[_T5],
            /,
            *,
            strict: bool = False,
        ) -> Self: ...
        @overload
        def __new__(
            cls,
            func: Callable[..., _S],
            iterable: Iterable[Any],
            iter2: Iterable[Any],
            iter3: Iterable[Any],
            iter4: Iterable[Any],
            iter5: Iterable[Any],
            iter6: Iterable[Any],
            /,
            *iterables: Iterable[Any],
            strict: bool = False,
        ) -> Self: ...
    else:
        @overload
        def __new__(cls, func: Callable[[_T1], _S], iterable: Iterable[_T1], /) -> Self: ...
        @overload
        def __new__(cls, func: Callable[[_T1, _T2], _S], iterable: Iterable[_T1], iter2: Iterable[_T2], /) -> Self: ...
        @overload
        def __new__(
            cls, func: Callable[[_T1, _T2, _T3], _S], iterable: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], /
        ) -> Self: ...
        @overload
        def __new__(
            cls,
            func: Callable[[_T1, _T2, _T3, _T4], _S],
            iterable: Iterable[_T1],
            iter2: Iterable[_T2],
            iter3: Iterable[_T3],
            iter4: Iterable[_T4],
            /,
        ) -> Self: ...
        @overload
        def __new__(
            cls,
            func: Callable[[_T1, _T2, _T3, _T4, _T5], _S],
            iterable: Iterable[_T1],
            iter2: Iterable[_T2],
            iter3: Iterable[_T3],
            iter4: Iterable[_T4],
            iter5: Iterable[_T5],
            /,
        ) -> Self: ...
        @overload
        def __new__(
            cls,
            func: Callable[..., _S],
            iterable: Iterable[Any],
            iter2: Iterable[Any],
            iter3: Iterable[Any],
            iter4: Iterable[Any],
            iter5: Iterable[Any],
            iter6: Iterable[Any],
            /,
            *iterables: Iterable[Any],
        ) -> Self: ...

    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _S:
        """Implement next(self)."""

@overload
def max(
    arg1: SupportsRichComparisonT, arg2: SupportsRichComparisonT, /, *_args: SupportsRichComparisonT, key: None = None
) -> SupportsRichComparisonT:
    """max(iterable, *[, default=obj, key=func]) -> value
    max(arg1, arg2, *args, *[, key=func]) -> value

    With a single iterable argument, return its biggest item. The
    default keyword-only argument specifies an object to return if
    the provided iterable is empty.
    With two or more positional arguments, return the largest argument.
    """

@overload
def max(arg1: _T, arg2: _T, /, *_args: _T, key: Callable[[_T], SupportsRichComparison]) -> _T: ...
@overload
def max(iterable: Iterable[SupportsRichComparisonT], /, *, key: None = None) -> SupportsRichComparisonT: ...
@overload
def max(iterable: Iterable[_T], /, *, key: Callable[[_T], SupportsRichComparison]) -> _T: ...
@overload
def max(iterable: Iterable[SupportsRichComparisonT], /, *, key: None = None, default: _T) -> SupportsRichComparisonT | _T: ...
@overload
def max(iterable: Iterable[_T1], /, *, key: Callable[[_T1], SupportsRichComparison], default: _T2) -> _T1 | _T2: ...
@overload
def min(
    arg1: SupportsRichComparisonT, arg2: SupportsRichComparisonT, /, *_args: SupportsRichComparisonT, key: None = None
) -> SupportsRichComparisonT:
    """min(iterable, *[, default=obj, key=func]) -> value
    min(arg1, arg2, *args, *[, key=func]) -> value

    With a single iterable argument, return its smallest item. The
    default keyword-only argument specifies an object to return if
    the provided iterable is empty.
    With two or more positional arguments, return the smallest argument.
    """

@overload
def min(arg1: _T, arg2: _T, /, *_args: _T, key: Callable[[_T], SupportsRichComparison]) -> _T: ...
@overload
def min(iterable: Iterable[SupportsRichComparisonT], /, *, key: None = None) -> SupportsRichComparisonT: ...
@overload
def min(iterable: Iterable[_T], /, *, key: Callable[[_T], SupportsRichComparison]) -> _T: ...
@overload
def min(iterable: Iterable[SupportsRichComparisonT], /, *, key: None = None, default: _T) -> SupportsRichComparisonT | _T: ...
@overload
def min(iterable: Iterable[_T1], /, *, key: Callable[[_T1], SupportsRichComparison], default: _T2) -> _T1 | _T2: ...
@overload
def next(i: SupportsNext[_T], /) -> _T:
    """next(iterator[, default])

    Return the next item from the iterator. If default is given and the iterator
    is exhausted, it is returned instead of raising StopIteration.
    """

@overload
def next(i: SupportsNext[_T], default: _VT, /) -> _T | _VT: ...
def oct(number: int | SupportsIndex, /) -> str:
    """Return the octal representation of an integer.

    >>> oct(342391)
    '0o1234567'
    """

_Opener: TypeAlias = Callable[[str, int], int]

# Text mode: always returns a TextIOWrapper
@overload
def open(
    file: FileDescriptorOrPath,
    mode: OpenTextMode = "r",
    buffering: int = -1,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
    closefd: bool = True,
    opener: _Opener | None = None,
) -> TextIOWrapper:
    """Open file and return a stream.  Raise OSError upon failure.

    file is either a text or byte string giving the name (and the path
    if the file isn't in the current working directory) of the file to
    be opened or an integer file descriptor of the file to be
    wrapped. (If a file descriptor is given, it is closed when the
    returned I/O object is closed, unless closefd is set to False.)

    mode is an optional string that specifies the mode in which the file
    is opened. It defaults to 'r' which means open for reading in text
    mode.  Other common values are 'w' for writing (truncating the file if
    it already exists), 'x' for creating and writing to a new file, and
    'a' for appending (which on some Unix systems, means that all writes
    append to the end of the file regardless of the current seek position).
    In text mode, if encoding is not specified the encoding used is platform
    dependent: locale.getencoding() is called to get the current locale encoding.
    (For reading and writing raw bytes use binary mode and leave encoding
    unspecified.) The available modes are:

    ========= ===============================================================
    Character Meaning
    --------- ---------------------------------------------------------------
    'r'       open for reading (default)
    'w'       open for writing, truncating the file first
    'x'       create a new file and open it for writing
    'a'       open for writing, appending to the end of the file if it exists
    'b'       binary mode
    't'       text mode (default)
    '+'       open a disk file for updating (reading and writing)
    ========= ===============================================================

    The default mode is 'rt' (open for reading text). For binary random
    access, the mode 'w+b' opens and truncates the file to 0 bytes, while
    'r+b' opens the file without truncation. The 'x' mode implies 'w' and
    raises an `FileExistsError` if the file already exists.

    Python distinguishes between files opened in binary and text modes,
    even when the underlying operating system doesn't. Files opened in
    binary mode (appending 'b' to the mode argument) return contents as
    bytes objects without any decoding. In text mode (the default, or when
    't' is appended to the mode argument), the contents of the file are
    returned as strings, the bytes having been first decoded using a
    platform-dependent encoding or using the specified encoding if given.

    buffering is an optional integer used to set the buffering policy.
    Pass 0 to switch buffering off (only allowed in binary mode), 1 to select
    line buffering (only usable in text mode), and an integer > 1 to indicate
    the size of a fixed-size chunk buffer.  When no buffering argument is
    given, the default buffering policy works as follows:

    * Binary files are buffered in fixed-size chunks; the size of the buffer
     is max(min(blocksize, 8 MiB), DEFAULT_BUFFER_SIZE)
     when the device block size is available.
     On most systems, the buffer will typically be 128 kilobytes long.

    * "Interactive" text files (files for which isatty() returns True)
      use line buffering.  Other text files use the policy described above
      for binary files.

    encoding is the name of the encoding used to decode or encode the
    file. This should only be used in text mode. The default encoding is
    platform dependent, but any encoding supported by Python can be
    passed.  See the codecs module for the list of supported encodings.

    errors is an optional string that specifies how encoding errors are to
    be handled---this argument should not be used in binary mode. Pass
    'strict' to raise a ValueError exception if there is an encoding error
    (the default of None has the same effect), or pass 'ignore' to ignore
    errors. (Note that ignoring encoding errors can lead to data loss.)
    See the documentation for codecs.register or run 'help(codecs.Codec)'
    for a list of the permitted encoding error strings.

    newline controls how universal newlines works (it only applies to text
    mode). It can be None, '', '\\n', '\\r', and '\\r\\n'.  It works as
    follows:

    * On input, if newline is None, universal newlines mode is
      enabled. Lines in the input can end in '\\n', '\\r', or '\\r\\n', and
      these are translated into '\\n' before being returned to the
      caller. If it is '', universal newline mode is enabled, but line
      endings are returned to the caller untranslated. If it has any of
      the other legal values, input lines are only terminated by the given
      string, and the line ending is returned to the caller untranslated.

    * On output, if newline is None, any '\\n' characters written are
      translated to the system default line separator, os.linesep. If
      newline is '' or '\\n', no translation takes place. If newline is any
      of the other legal values, any '\\n' characters written are translated
      to the given string.

    If closefd is False, the underlying file descriptor will be kept open
    when the file is closed. This does not work when a file name is given
    and must be True in that case.

    A custom opener can be used by passing a callable as *opener*. The
    underlying file descriptor for the file object is then obtained by
    calling *opener* with (*file*, *flags*). *opener* must return an open
    file descriptor (passing os.open as *opener* results in functionality
    similar to passing None).

    open() returns a file object whose type depends on the mode, and
    through which the standard file operations such as reading and writing
    are performed. When open() is used to open a file in a text mode ('w',
    'r', 'wt', 'rt', etc.), it returns a TextIOWrapper. When used to open
    a file in a binary mode, the returned class varies: in read binary
    mode, it returns a BufferedReader; in write binary and append binary
    modes, it returns a BufferedWriter, and in read/write mode, it returns
    a BufferedRandom.

    It is also possible to use a string or bytearray as a file for both
    reading and writing. For strings StringIO can be used like a file
    opened in a text mode, and for bytes a BytesIO can be used like a file
    opened in a binary mode.
    """

# Unbuffered binary mode: returns a FileIO
@overload
def open(
    file: FileDescriptorOrPath,
    mode: OpenBinaryMode,
    buffering: Literal[0],
    encoding: None = None,
    errors: None = None,
    newline: None = None,
    closefd: bool = True,
    opener: _Opener | None = None,
) -> FileIO: ...

# Buffering is on: return BufferedRandom, BufferedReader, or BufferedWriter
@overload
def open(
    file: FileDescriptorOrPath,
    mode: OpenBinaryModeUpdating,
    buffering: Literal[-1, 1] = -1,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
    closefd: bool = True,
    opener: _Opener | None = None,
) -> BufferedRandom: ...
@overload
def open(
    file: FileDescriptorOrPath,
    mode: OpenBinaryModeWriting,
    buffering: Literal[-1, 1] = -1,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
    closefd: bool = True,
    opener: _Opener | None = None,
) -> BufferedWriter: ...
@overload
def open(
    file: FileDescriptorOrPath,
    mode: OpenBinaryModeReading,
    buffering: Literal[-1, 1] = -1,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
    closefd: bool = True,
    opener: _Opener | None = None,
) -> BufferedReader: ...

# Buffering cannot be determined: fall back to BinaryIO
@overload
def open(
    file: FileDescriptorOrPath,
    mode: OpenBinaryMode,
    buffering: int = -1,
    encoding: None = None,
    errors: None = None,
    newline: None = None,
    closefd: bool = True,
    opener: _Opener | None = None,
) -> BinaryIO: ...

# Fallback if mode is not specified
@overload
def open(
    file: FileDescriptorOrPath,
    mode: str,
    buffering: int = -1,
    encoding: str | None = None,
    errors: str | None = None,
    newline: str | None = None,
    closefd: bool = True,
    opener: _Opener | None = None,
) -> IO[Any]: ...
def ord(c: str | bytes | bytearray, /) -> int:
    """Return the ordinal value of a character.

    If the argument is a one-character string, return the Unicode code
    point of that character.

    If the argument is a bytes or bytearray object of length 1, return its
    single byte value.
    """

@type_check_only
class _SupportsWriteAndFlush(SupportsWrite[_T_contra], SupportsFlush, Protocol[_T_contra]): ...

@overload
def print(
    *values: object,
    sep: str | None = " ",
    end: str | None = "\n",
    file: SupportsWrite[str] | None = None,
    flush: Literal[False] = False,
) -> None:
    """Prints the values to a stream, or to sys.stdout by default.

    sep
      string inserted between values, default a space.
    end
      string appended after the last value, default a newline.
    file
      a file-like object (stream); defaults to the current sys.stdout.
    flush
      whether to forcibly flush the stream.
    """

@overload
def print(
    *values: object, sep: str | None = " ", end: str | None = "\n", file: _SupportsWriteAndFlush[str] | None = None, flush: bool
) -> None: ...

_E_contra = TypeVar("_E_contra", contravariant=True)
_M_contra = TypeVar("_M_contra", contravariant=True)

@type_check_only
class _SupportsPow2(Protocol[_E_contra, _T_co]):
    def __pow__(self, other: _E_contra, /) -> _T_co: ...

@type_check_only
class _SupportsPow3NoneOnly(Protocol[_E_contra, _T_co]):
    def __pow__(self, other: _E_contra, modulo: None = None, /) -> _T_co: ...

@type_check_only
class _SupportsPow3(Protocol[_E_contra, _M_contra, _T_co]):
    def __pow__(self, other: _E_contra, modulo: _M_contra, /) -> _T_co: ...

_SupportsSomeKindOfPow = (  # noqa: Y026  # TODO: Use TypeAlias once mypy bugs are fixed
    _SupportsPow2[Any, Any] | _SupportsPow3NoneOnly[Any, Any] | _SupportsPow3[Any, Any, Any]
)

# TODO: `pow(int, int, Literal[0])` fails at runtime,
# but adding a `NoReturn` overload isn't a good solution for expressing that (see #8566).
@overload
def pow(base: int, exp: int, mod: int) -> int:
    """Equivalent to base**exp with 2 arguments or base**exp % mod with 3 arguments

    Some types, such as ints, are able to use a more efficient algorithm when
    invoked using the three argument form.
    """

@overload
def pow(base: int, exp: Literal[0], mod: None = None) -> Literal[1]: ...
@overload
def pow(base: int, exp: _PositiveInteger, mod: None = None) -> int: ...
@overload
def pow(base: int, exp: _NegativeInteger, mod: None = None) -> float: ...

# int base & positive-int exp -> int; int base & negative-int exp -> float
# return type must be Any as `int | float` causes too many false-positive errors
@overload
def pow(base: int, exp: int, mod: None = None) -> Any: ...
@overload
def pow(base: _PositiveInteger, exp: float, mod: None = None) -> float: ...
@overload
def pow(base: _NegativeInteger, exp: float, mod: None = None) -> complex: ...
@overload
def pow(base: float, exp: int, mod: None = None) -> float: ...

# float base & float exp could return float or complex
# return type must be Any (same as complex base, complex exp),
# as `float | complex` causes too many false-positive errors
@overload
def pow(base: float, exp: complex | _SupportsSomeKindOfPow, mod: None = None) -> Any: ...
@overload
def pow(base: complex, exp: complex | _SupportsSomeKindOfPow, mod: None = None) -> complex: ...
@overload
def pow(base: _SupportsPow2[_E_contra, _T_co], exp: _E_contra, mod: None = None) -> _T_co: ...  # type: ignore[overload-overlap]
@overload
def pow(base: _SupportsPow3NoneOnly[_E_contra, _T_co], exp: _E_contra, mod: None = None) -> _T_co: ...  # type: ignore[overload-overlap]
@overload
def pow(base: _SupportsPow3[_E_contra, _M_contra, _T_co], exp: _E_contra, mod: _M_contra) -> _T_co: ...
@overload
def pow(base: _SupportsSomeKindOfPow, exp: float, mod: None = None) -> Any: ...
@overload
def pow(base: _SupportsSomeKindOfPow, exp: complex, mod: None = None) -> complex: ...

quit: _sitebuiltins.Quitter

@disjoint_base
class reversed(Generic[_T]):
    """Return a reverse iterator over the values of the given sequence."""

    @overload
    def __new__(cls, sequence: Reversible[_T], /) -> Iterator[_T]: ...  # type: ignore[misc]
    @overload
    def __new__(cls, sequence: SupportsLenAndGetItem[_T], /) -> Iterator[_T]: ...  # type: ignore[misc]
    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T:
        """Implement next(self)."""

    def __length_hint__(self) -> int:
        """Private method returning an estimate of len(list(it))."""

def repr(obj: object, /) -> str:
    """Return the canonical string representation of the object.

    For many object types, including most builtins, eval(repr(obj)) == obj.
    """

# See https://github.com/python/typeshed/pull/9141
# and https://github.com/python/typeshed/pull/9151
# on why we don't use `SupportsRound` from `typing.pyi`

@type_check_only
class _SupportsRound1(Protocol[_T_co]):
    def __round__(self) -> _T_co: ...

@type_check_only
class _SupportsRound2(Protocol[_T_co]):
    def __round__(self, ndigits: int, /) -> _T_co: ...

@overload
def round(number: _SupportsRound1[_T], ndigits: None = None) -> _T:
    """Round a number to a given precision in decimal digits.

    The return value is an integer if ndigits is omitted or None.  Otherwise
    the return value has the same type as the number.  ndigits may be negative.
    """

@overload
def round(number: _SupportsRound2[_T], ndigits: SupportsIndex) -> _T: ...

# See https://github.com/python/typeshed/pull/6292#discussion_r748875189
# for why arg 3 of `setattr` should be annotated with `Any` and not `object`
def setattr(obj: object, name: str, value: Any, /) -> None:
    """Sets the named attribute on the given object to the specified value.

    setattr(x, 'y', v) is equivalent to ``x.y = v``
    """

@overload
def sorted(
    iterable: Iterable[SupportsRichComparisonT], /, *, key: None = None, reverse: bool = False
) -> list[SupportsRichComparisonT]:
    """Return a new list containing all items from the iterable in ascending order.

    A custom key function can be supplied to customize the sort order, and the
    reverse flag can be set to request the result in descending order.
    """

@overload
def sorted(iterable: Iterable[_T], /, *, key: Callable[[_T], SupportsRichComparison], reverse: bool = False) -> list[_T]: ...

_AddableT1 = TypeVar("_AddableT1", bound=SupportsAdd[Any, Any])
_AddableT2 = TypeVar("_AddableT2", bound=SupportsAdd[Any, Any])

@type_check_only
class _SupportsSumWithNoDefaultGiven(SupportsAdd[Any, Any], SupportsRAdd[int, Any], Protocol): ...

_SupportsSumNoDefaultT = TypeVar("_SupportsSumNoDefaultT", bound=_SupportsSumWithNoDefaultGiven)

# In general, the return type of `x + x` is *not* guaranteed to be the same type as x.
# However, we can't express that in the stub for `sum()`
# without creating many false-positive errors (see #7578).
# Instead, we special-case the most common examples of this: bool and literal integers.
@overload
def sum(iterable: Iterable[bool | _LiteralInteger], /, start: int = 0) -> int:
    """Return the sum of a 'start' value (default: 0) plus an iterable of numbers

    When the iterable is empty, return the start value.
    This function is intended specifically for use with numeric values and may
    reject non-numeric types.
    """

@overload
def sum(iterable: Iterable[_SupportsSumNoDefaultT], /) -> _SupportsSumNoDefaultT | Literal[0]: ...
@overload
def sum(iterable: Iterable[_AddableT1], /, start: _AddableT2) -> _AddableT1 | _AddableT2: ...

# The argument to `vars()` has to have a `__dict__` attribute, so the second overload can't be annotated with `object`
# (A "SupportsDunderDict" protocol doesn't work)
@overload
def vars(object: type, /) -> types.MappingProxyType[str, Any]:
    """vars([object]) -> dictionary

    Without arguments, equivalent to locals().
    With an argument, equivalent to object.__dict__.
    """

@overload
def vars(object: Any = ..., /) -> dict[str, Any]: ...
@disjoint_base
class zip(Generic[_T_co]):
    """The zip object yields n-length tuples, where n is the number of iterables
    passed as positional arguments to zip().  The i-th element in every tuple
    comes from the i-th iterable argument to zip().  This continues until the
    shortest argument is exhausted.

    If strict is true and one of the arguments is exhausted before the others,
    raise a ValueError.

       >>> list(zip('abcdefg', range(3), range(4)))
       [('a', 0, 0), ('b', 1, 1), ('c', 2, 2)]
    """

    if sys.version_info >= (3, 10):
        @overload
        def __new__(cls, *, strict: bool = False) -> zip[Any]: ...
        @overload
        def __new__(cls, iter1: Iterable[_T1], /, *, strict: bool = False) -> zip[tuple[_T1]]: ...
        @overload
        def __new__(cls, iter1: Iterable[_T1], iter2: Iterable[_T2], /, *, strict: bool = False) -> zip[tuple[_T1, _T2]]: ...
        @overload
        def __new__(
            cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], /, *, strict: bool = False
        ) -> zip[tuple[_T1, _T2, _T3]]: ...
        @overload
        def __new__(
            cls,
            iter1: Iterable[_T1],
            iter2: Iterable[_T2],
            iter3: Iterable[_T3],
            iter4: Iterable[_T4],
            /,
            *,
            strict: bool = False,
        ) -> zip[tuple[_T1, _T2, _T3, _T4]]: ...
        @overload
        def __new__(
            cls,
            iter1: Iterable[_T1],
            iter2: Iterable[_T2],
            iter3: Iterable[_T3],
            iter4: Iterable[_T4],
            iter5: Iterable[_T5],
            /,
            *,
            strict: bool = False,
        ) -> zip[tuple[_T1, _T2, _T3, _T4, _T5]]: ...
        @overload
        def __new__(
            cls,
            iter1: Iterable[Any],
            iter2: Iterable[Any],
            iter3: Iterable[Any],
            iter4: Iterable[Any],
            iter5: Iterable[Any],
            iter6: Iterable[Any],
            /,
            *iterables: Iterable[Any],
            strict: bool = False,
        ) -> zip[tuple[Any, ...]]: ...
    else:
        @overload
        def __new__(cls) -> zip[Any]: ...
        @overload
        def __new__(cls, iter1: Iterable[_T1], /) -> zip[tuple[_T1]]: ...
        @overload
        def __new__(cls, iter1: Iterable[_T1], iter2: Iterable[_T2], /) -> zip[tuple[_T1, _T2]]: ...
        @overload
        def __new__(cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], /) -> zip[tuple[_T1, _T2, _T3]]: ...
        @overload
        def __new__(
            cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], iter4: Iterable[_T4], /
        ) -> zip[tuple[_T1, _T2, _T3, _T4]]: ...
        @overload
        def __new__(
            cls, iter1: Iterable[_T1], iter2: Iterable[_T2], iter3: Iterable[_T3], iter4: Iterable[_T4], iter5: Iterable[_T5], /
        ) -> zip[tuple[_T1, _T2, _T3, _T4, _T5]]: ...
        @overload
        def __new__(
            cls,
            iter1: Iterable[Any],
            iter2: Iterable[Any],
            iter3: Iterable[Any],
            iter4: Iterable[Any],
            iter5: Iterable[Any],
            iter6: Iterable[Any],
            /,
            *iterables: Iterable[Any],
        ) -> zip[tuple[Any, ...]]: ...

    def __iter__(self) -> Self:
        """Implement iter(self)."""

    def __next__(self) -> _T_co:
        """Implement next(self)."""

# Signature of `builtins.__import__` should be kept identical to `importlib.__import__`
# Return type of `__import__` should be kept the same as return type of `importlib.import_module`
def __import__(
    name: str,
    globals: Mapping[str, object] | None = None,
    locals: Mapping[str, object] | None = None,
    fromlist: Sequence[str] | None = (),
    level: int = 0,
) -> types.ModuleType:
    """Import a module.

    Because this function is meant for use by the Python
    interpreter and not for general use, it is better to use
    importlib.import_module() to programmatically import a module.

    The globals argument is only used to determine the context;
    they are not modified.  The locals argument is unused.  The fromlist
    should be a list of names to emulate ``from name import ...``, or an
    empty list to emulate ``import name``.
    When importing a module from a package, note that __import__('A.B', ...)
    returns package A when fromlist is empty, but its submodule B when
    fromlist is not empty.  The level argument is used to determine whether to
    perform absolute or relative imports: 0 is absolute, while a positive number
    is the number of parent directories to search relative to the current module.
    """

def __build_class__(func: Callable[[], CellType | Any], name: str, /, *bases: Any, metaclass: Any = ..., **kwds: Any) -> Any:
    """__build_class__(func, name, /, *bases, [metaclass], **kwds) -> class

    Internal helper function used by the class statement.
    """

if sys.version_info >= (3, 10):
    from types import EllipsisType, NotImplementedType

    # Backwards compatibility hack for folks who relied on the ellipsis type
    # existing in typeshed in Python 3.9 and earlier.
    ellipsis = EllipsisType

    Ellipsis: EllipsisType
    NotImplemented: NotImplementedType
else:
    # Actually the type of Ellipsis is <type 'ellipsis'>, but since it's
    # not exposed anywhere under that name, we make it private here.
    @final
    @type_check_only
    class ellipsis: ...

    Ellipsis: ellipsis

    @final
    @type_check_only
    class _NotImplementedType(Any): ...

    NotImplemented: _NotImplementedType

@disjoint_base
class BaseException:
    """Common base class for all exceptions"""

    args: tuple[Any, ...]
    __cause__: BaseException | None
    __context__: BaseException | None
    __suppress_context__: bool
    __traceback__: TracebackType | None
    def __init__(self, *args: object) -> None: ...
    def __new__(cls, *args: Any, **kwds: Any) -> Self: ...
    def __setstate__(self, state: dict[str, Any] | None, /) -> None: ...
    def with_traceback(self, tb: TracebackType | None, /) -> Self:
        """Set self.__traceback__ to tb and return self."""
    # Necessary for security-focused static analyzers (e.g, pysa)
    # See https://github.com/python/typeshed/pull/14900
    def __str__(self) -> str: ...  # noqa: Y029
    def __repr__(self) -> str: ...  # noqa: Y029
    if sys.version_info >= (3, 11):
        # only present after add_note() is called
        __notes__: list[str]
        def add_note(self, note: str, /) -> None:
            """Add a note to the exception"""

class GeneratorExit(BaseException):
    """Request that a generator exit."""

class KeyboardInterrupt(BaseException):
    """Program interrupted by user."""

@disjoint_base
class SystemExit(BaseException):
    """Request to exit from the interpreter."""

    code: sys._ExitCode

class Exception(BaseException):
    """Common base class for all non-exit exceptions."""

@disjoint_base
class StopIteration(Exception):
    """Signal the end from iterator.__next__()."""

    value: Any

@disjoint_base
class OSError(Exception):
    """Base class for I/O related errors."""

    errno: int | None
    strerror: str | None
    # filename, filename2 are actually str | bytes | None
    filename: Any
    filename2: Any
    if sys.platform == "win32":
        winerror: int

EnvironmentError = OSError
IOError = OSError
if sys.platform == "win32":
    WindowsError = OSError

class ArithmeticError(Exception):
    """Base class for arithmetic errors."""

class AssertionError(Exception):
    """Assertion failed."""

if sys.version_info >= (3, 10):
    @disjoint_base
    class AttributeError(Exception):
        """Attribute not found."""

        def __init__(self, *args: object, name: str | None = None, obj: object = None) -> None: ...
        name: str | None
        obj: object

else:
    class AttributeError(Exception):
        """Attribute not found."""

class BufferError(Exception):
    """Buffer error."""

class EOFError(Exception):
    """Read beyond end of file."""

@disjoint_base
class ImportError(Exception):
    """Import can't find module, or can't find name in module."""

    def __init__(self, *args: object, name: str | None = None, path: str | None = None) -> None: ...
    name: str | None
    path: str | None
    msg: str  # undocumented
    if sys.version_info >= (3, 12):
        name_from: str | None  # undocumented

class LookupError(Exception):
    """Base class for lookup errors."""

class MemoryError(Exception):
    """Out of memory."""

if sys.version_info >= (3, 10):
    @disjoint_base
    class NameError(Exception):
        """Name not found globally."""

        def __init__(self, *args: object, name: str | None = None) -> None: ...
        name: str | None

else:
    class NameError(Exception):
        """Name not found globally."""

class ReferenceError(Exception):
    """Weak ref proxy used after referent went away."""

class RuntimeError(Exception):
    """Unspecified run-time error."""

class StopAsyncIteration(Exception):
    """Signal the end from iterator.__anext__()."""

@disjoint_base
class SyntaxError(Exception):
    """Invalid syntax."""

    msg: str
    filename: str | None
    lineno: int | None
    offset: int | None
    text: str | None
    # Errors are displayed differently if this attribute exists on the exception.
    # The value is always None.
    print_file_and_line: None
    if sys.version_info >= (3, 10):
        end_lineno: int | None
        end_offset: int | None

    @overload
    def __init__(self) -> None: ...
    @overload
    def __init__(self, msg: object, /) -> None: ...
    # Second argument is the tuple (filename, lineno, offset, text)
    @overload
    def __init__(self, msg: str, info: tuple[str | None, int | None, int | None, str | None], /) -> None: ...
    if sys.version_info >= (3, 10):
        # end_lineno and end_offset must both be provided if one is.
        @overload
        def __init__(
            self, msg: str, info: tuple[str | None, int | None, int | None, str | None, int | None, int | None], /
        ) -> None: ...
    # If you provide more than two arguments, it still creates the SyntaxError, but
    # the arguments from the info tuple are not parsed. This form is omitted.

class SystemError(Exception):
    """Internal error in the Python interpreter.

    Please report this to the Python maintainer, along with the traceback,
    the Python version, and the hardware/OS platform and version.
    """

class TypeError(Exception):
    """Inappropriate argument type."""

class ValueError(Exception):
    """Inappropriate argument value (of correct type)."""

class FloatingPointError(ArithmeticError):
    """Floating-point operation failed."""

class OverflowError(ArithmeticError):
    """Result too large to be represented."""

class ZeroDivisionError(ArithmeticError):
    """Second argument to a division or modulo operation was zero."""

class ModuleNotFoundError(ImportError):
    """Module not found."""

class IndexError(LookupError):
    """Sequence index out of range."""

class KeyError(LookupError):
    """Mapping key not found."""

class UnboundLocalError(NameError):
    """Local name referenced but not bound to a value."""

class BlockingIOError(OSError):
    """I/O operation would block."""

    characters_written: int

class ChildProcessError(OSError):
    """Child process error."""

class ConnectionError(OSError):
    """Connection error."""

class BrokenPipeError(ConnectionError):
    """Broken pipe."""

class ConnectionAbortedError(ConnectionError):
    """Connection aborted."""

class ConnectionRefusedError(ConnectionError):
    """Connection refused."""

class ConnectionResetError(ConnectionError):
    """Connection reset."""

class FileExistsError(OSError):
    """File already exists."""

class FileNotFoundError(OSError):
    """File not found."""

class InterruptedError(OSError):
    """Interrupted by signal."""

class IsADirectoryError(OSError):
    """Operation doesn't work on directories."""

class NotADirectoryError(OSError):
    """Operation only works on directories."""

class PermissionError(OSError):
    """Not enough permissions."""

class ProcessLookupError(OSError):
    """Process not found."""

class TimeoutError(OSError):
    """Timeout expired."""

class NotImplementedError(RuntimeError):
    """Method or function hasn't been implemented yet."""

class RecursionError(RuntimeError):
    """Recursion limit exceeded."""

class IndentationError(SyntaxError):
    """Improper indentation."""

class TabError(IndentationError):
    """Improper mixture of spaces and tabs."""

class UnicodeError(ValueError):
    """Unicode related error."""

@disjoint_base
class UnicodeDecodeError(UnicodeError):
    """Unicode decoding error."""

    encoding: str
    object: bytes
    start: int
    end: int
    reason: str
    def __init__(self, encoding: str, object: ReadableBuffer, start: int, end: int, reason: str, /) -> None: ...

@disjoint_base
class UnicodeEncodeError(UnicodeError):
    """Unicode encoding error."""

    encoding: str
    object: str
    start: int
    end: int
    reason: str
    def __init__(self, encoding: str, object: str, start: int, end: int, reason: str, /) -> None: ...

@disjoint_base
class UnicodeTranslateError(UnicodeError):
    """Unicode translation error."""

    encoding: None
    object: str
    start: int
    end: int
    reason: str
    def __init__(self, object: str, start: int, end: int, reason: str, /) -> None: ...

class Warning(Exception):
    """Base class for warning categories."""

class UserWarning(Warning):
    """Base class for warnings generated by user code."""

class DeprecationWarning(Warning):
    """Base class for warnings about deprecated features."""

class SyntaxWarning(Warning):
    """Base class for warnings about dubious syntax."""

class RuntimeWarning(Warning):
    """Base class for warnings about dubious runtime behavior."""

class FutureWarning(Warning):
    """Base class for warnings about constructs that will change semantically
    in the future.
    """

class PendingDeprecationWarning(Warning):
    """Base class for warnings about features which will be deprecated
    in the future.
    """

class ImportWarning(Warning):
    """Base class for warnings about probable mistakes in module imports"""

class UnicodeWarning(Warning):
    """Base class for warnings about Unicode related problems, mostly
    related to conversion problems.
    """

class BytesWarning(Warning):
    """Base class for warnings about bytes and buffer related problems, mostly
    related to conversion from str or comparing to str.
    """

class ResourceWarning(Warning):
    """Base class for warnings about resource usage."""

if sys.version_info >= (3, 10):
    class EncodingWarning(Warning):
        """Base class for warnings about encodings."""

if sys.version_info >= (3, 11):
    _BaseExceptionT_co = TypeVar("_BaseExceptionT_co", bound=BaseException, covariant=True, default=BaseException)
    _BaseExceptionT = TypeVar("_BaseExceptionT", bound=BaseException)
    _ExceptionT_co = TypeVar("_ExceptionT_co", bound=Exception, covariant=True, default=Exception)
    _ExceptionT = TypeVar("_ExceptionT", bound=Exception)

    # See `check_exception_group.py` for use-cases and comments.
    @disjoint_base
    class BaseExceptionGroup(BaseException, Generic[_BaseExceptionT_co]):
        """A combination of multiple unrelated exceptions."""

        def __new__(cls, message: str, exceptions: Sequence[_BaseExceptionT_co], /) -> Self: ...
        def __init__(self, message: str, exceptions: Sequence[_BaseExceptionT_co], /) -> None: ...
        @property
        def message(self) -> str:
            """exception message"""

        @property
        def exceptions(self) -> tuple[_BaseExceptionT_co | BaseExceptionGroup[_BaseExceptionT_co], ...]:
            """nested exceptions"""

        @overload
        def subgroup(
            self, matcher_value: type[_ExceptionT] | tuple[type[_ExceptionT], ...], /
        ) -> ExceptionGroup[_ExceptionT] | None: ...
        @overload
        def subgroup(
            self, matcher_value: type[_BaseExceptionT] | tuple[type[_BaseExceptionT], ...], /
        ) -> BaseExceptionGroup[_BaseExceptionT] | None: ...
        @overload
        def subgroup(
            self, matcher_value: Callable[[_BaseExceptionT_co | Self], bool], /
        ) -> BaseExceptionGroup[_BaseExceptionT_co] | None: ...
        @overload
        def split(
            self, matcher_value: type[_ExceptionT] | tuple[type[_ExceptionT], ...], /
        ) -> tuple[ExceptionGroup[_ExceptionT] | None, BaseExceptionGroup[_BaseExceptionT_co] | None]: ...
        @overload
        def split(
            self, matcher_value: type[_BaseExceptionT] | tuple[type[_BaseExceptionT], ...], /
        ) -> tuple[BaseExceptionGroup[_BaseExceptionT] | None, BaseExceptionGroup[_BaseExceptionT_co] | None]: ...
        @overload
        def split(
            self, matcher_value: Callable[[_BaseExceptionT_co | Self], bool], /
        ) -> tuple[BaseExceptionGroup[_BaseExceptionT_co] | None, BaseExceptionGroup[_BaseExceptionT_co] | None]: ...
        # In reality it is `NonEmptySequence`:
        @overload
        def derive(self, excs: Sequence[_ExceptionT], /) -> ExceptionGroup[_ExceptionT]: ...
        @overload
        def derive(self, excs: Sequence[_BaseExceptionT], /) -> BaseExceptionGroup[_BaseExceptionT]: ...
        def __class_getitem__(cls, item: Any, /) -> GenericAlias:
            """See PEP 585"""

    class ExceptionGroup(BaseExceptionGroup[_ExceptionT_co], Exception):
        def __new__(cls, message: str, exceptions: Sequence[_ExceptionT_co], /) -> Self: ...
        def __init__(self, message: str, exceptions: Sequence[_ExceptionT_co], /) -> None: ...
        @property
        def exceptions(self) -> tuple[_ExceptionT_co | ExceptionGroup[_ExceptionT_co], ...]:
            """nested exceptions"""
        # We accept a narrower type, but that's OK.
        @overload  # type: ignore[override]
        def subgroup(
            self, matcher_value: type[_ExceptionT] | tuple[type[_ExceptionT], ...], /
        ) -> ExceptionGroup[_ExceptionT] | None: ...
        @overload
        def subgroup(
            self, matcher_value: Callable[[_ExceptionT_co | Self], bool], /
        ) -> ExceptionGroup[_ExceptionT_co] | None: ...
        @overload  # type: ignore[override]
        def split(
            self, matcher_value: type[_ExceptionT] | tuple[type[_ExceptionT], ...], /
        ) -> tuple[ExceptionGroup[_ExceptionT] | None, ExceptionGroup[_ExceptionT_co] | None]: ...
        @overload
        def split(
            self, matcher_value: Callable[[_ExceptionT_co | Self], bool], /
        ) -> tuple[ExceptionGroup[_ExceptionT_co] | None, ExceptionGroup[_ExceptionT_co] | None]: ...

if sys.version_info >= (3, 13):
    class PythonFinalizationError(RuntimeError):
        """Operation blocked during Python finalization."""
