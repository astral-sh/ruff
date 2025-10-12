import enum
import sys
import types
from _typeshed import DataclassInstance
from builtins import type as Type  # alias to avoid name clashes with fields named "type"
from collections.abc import Callable, Iterable, Mapping
from types import GenericAlias
from typing import Any, Final, Generic, Literal, Protocol, TypeVar, overload, type_check_only
from typing_extensions import Never, TypeIs

_T = TypeVar("_T")
_T_co = TypeVar("_T_co", covariant=True)

__all__ = [
    "dataclass",
    "field",
    "Field",
    "FrozenInstanceError",
    "InitVar",
    "MISSING",
    "fields",
    "asdict",
    "astuple",
    "make_dataclass",
    "replace",
    "is_dataclass",
]

if sys.version_info >= (3, 10):
    __all__ += ["KW_ONLY"]

_DataclassT = TypeVar("_DataclassT", bound=DataclassInstance)

@type_check_only
class _DataclassFactory(Protocol):
    def __call__(
        self,
        cls: type[_T],
        /,
        *,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
        match_args: bool = True,
        kw_only: bool = False,
        slots: bool = False,
        weakref_slot: bool = False,
    ) -> type[_T]: ...

# define _MISSING_TYPE as an enum within the type stubs,
# even though that is not really its type at runtime
# this allows us to use Literal[_MISSING_TYPE.MISSING]
# for background, see:
#   https://github.com/python/typeshed/pull/5900#issuecomment-895513797
class _MISSING_TYPE(enum.Enum):
    MISSING = enum.auto()

MISSING: Final = _MISSING_TYPE.MISSING

if sys.version_info >= (3, 10):
    class KW_ONLY: ...

@overload
def asdict(obj: DataclassInstance) -> dict[str, Any]:
    """Return the fields of a dataclass instance as a new dictionary mapping
    field names to field values.

    Example usage::

      @dataclass
      class C:
          x: int
          y: int

      c = C(1, 2)
      assert asdict(c) == {'x': 1, 'y': 2}

    If given, 'dict_factory' will be used instead of built-in dict.
    The function applies recursively to field values that are
    dataclass instances. This will also look into built-in containers:
    tuples, lists, and dicts. Other objects are copied with 'copy.deepcopy()'.
    """

@overload
def asdict(obj: DataclassInstance, *, dict_factory: Callable[[list[tuple[str, Any]]], _T]) -> _T: ...
@overload
def astuple(obj: DataclassInstance) -> tuple[Any, ...]:
    """Return the fields of a dataclass instance as a new tuple of field values.

    Example usage::

      @dataclass
      class C:
          x: int
          y: int

      c = C(1, 2)
      assert astuple(c) == (1, 2)

    If given, 'tuple_factory' will be used instead of built-in tuple.
    The function applies recursively to field values that are
    dataclass instances. This will also look into built-in containers:
    tuples, lists, and dicts. Other objects are copied with 'copy.deepcopy()'.
    """

@overload
def astuple(obj: DataclassInstance, *, tuple_factory: Callable[[list[Any]], _T]) -> _T: ...

if sys.version_info >= (3, 11):
    @overload
    def dataclass(
        cls: type[_T],
        /,
        *,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
        match_args: bool = True,
        kw_only: bool = False,
        slots: bool = False,
        weakref_slot: bool = False,
    ) -> type[_T]:
        """Add dunder methods based on the fields defined in the class.

        Examines PEP 526 __annotations__ to determine fields.

        If init is true, an __init__() method is added to the class. If repr
        is true, a __repr__() method is added. If order is true, rich
        comparison dunder methods are added. If unsafe_hash is true, a
        __hash__() method is added. If frozen is true, fields may not be
        assigned to after instance creation. If match_args is true, the
        __match_args__ tuple is added. If kw_only is true, then by default
        all fields are keyword-only. If slots is true, a new class with a
        __slots__ attribute is returned.
        """

    @overload
    def dataclass(
        cls: None = None,
        /,
        *,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
        match_args: bool = True,
        kw_only: bool = False,
        slots: bool = False,
        weakref_slot: bool = False,
    ) -> Callable[[type[_T]], type[_T]]: ...

elif sys.version_info >= (3, 10):
    @overload
    def dataclass(
        cls: type[_T],
        /,
        *,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
        match_args: bool = True,
        kw_only: bool = False,
        slots: bool = False,
    ) -> type[_T]:
        """Returns the same class as was passed in, with dunder methods
        added based on the fields defined in the class.

        Examines PEP 526 __annotations__ to determine fields.

        If init is true, an __init__() method is added to the class. If
        repr is true, a __repr__() method is added. If order is true, rich
        comparison dunder methods are added. If unsafe_hash is true, a
        __hash__() method function is added. If frozen is true, fields may
        not be assigned to after instance creation. If match_args is true,
        the __match_args__ tuple is added. If kw_only is true, then by
        default all fields are keyword-only. If slots is true, an
        __slots__ attribute is added.
        """

    @overload
    def dataclass(
        cls: None = None,
        /,
        *,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
        match_args: bool = True,
        kw_only: bool = False,
        slots: bool = False,
    ) -> Callable[[type[_T]], type[_T]]: ...

else:
    @overload
    def dataclass(
        cls: type[_T],
        /,
        *,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
    ) -> type[_T]:
        """Returns the same class as was passed in, with dunder methods
        added based on the fields defined in the class.

        Examines PEP 526 __annotations__ to determine fields.

        If init is true, an __init__() method is added to the class. If
        repr is true, a __repr__() method is added. If order is true, rich
        comparison dunder methods are added. If unsafe_hash is true, a
        __hash__() method function is added. If frozen is true, fields may
        not be assigned to after instance creation.
        """

    @overload
    def dataclass(
        cls: None = None,
        /,
        *,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
    ) -> Callable[[type[_T]], type[_T]]: ...

# See https://github.com/python/mypy/issues/10750
@type_check_only
class _DefaultFactory(Protocol[_T_co]):
    def __call__(self) -> _T_co: ...

class Field(Generic[_T]):
    if sys.version_info >= (3, 14):
        __slots__ = (
            "name",
            "type",
            "default",
            "default_factory",
            "repr",
            "hash",
            "init",
            "compare",
            "metadata",
            "kw_only",
            "doc",
            "_field_type",
        )
    elif sys.version_info >= (3, 10):
        __slots__ = (
            "name",
            "type",
            "default",
            "default_factory",
            "repr",
            "hash",
            "init",
            "compare",
            "metadata",
            "kw_only",
            "_field_type",
        )
    else:
        __slots__ = ("name", "type", "default", "default_factory", "repr", "hash", "init", "compare", "metadata", "_field_type")
    name: str
    type: Type[_T] | str | Any
    default: _T | Literal[_MISSING_TYPE.MISSING]
    default_factory: _DefaultFactory[_T] | Literal[_MISSING_TYPE.MISSING]
    repr: bool
    hash: bool | None
    init: bool
    compare: bool
    metadata: types.MappingProxyType[Any, Any]

    if sys.version_info >= (3, 14):
        doc: str | None

    if sys.version_info >= (3, 10):
        kw_only: bool | Literal[_MISSING_TYPE.MISSING]

    if sys.version_info >= (3, 14):
        def __init__(
            self,
            default: _T,
            default_factory: Callable[[], _T],
            init: bool,
            repr: bool,
            hash: bool | None,
            compare: bool,
            metadata: Mapping[Any, Any],
            kw_only: bool,
            doc: str | None,
        ) -> None: ...
    elif sys.version_info >= (3, 10):
        def __init__(
            self,
            default: _T,
            default_factory: Callable[[], _T],
            init: bool,
            repr: bool,
            hash: bool | None,
            compare: bool,
            metadata: Mapping[Any, Any],
            kw_only: bool,
        ) -> None: ...
    else:
        def __init__(
            self,
            default: _T,
            default_factory: Callable[[], _T],
            init: bool,
            repr: bool,
            hash: bool | None,
            compare: bool,
            metadata: Mapping[Any, Any],
        ) -> None: ...

    def __set_name__(self, owner: Type[Any], name: str) -> None: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

# NOTE: Actual return type is 'Field[_T]', but we want to help type checkers
# to understand the magic that happens at runtime.
if sys.version_info >= (3, 14):
    @overload  # `default` and `default_factory` are optional and mutually exclusive.
    def field(
        *,
        default: _T,
        default_factory: Literal[_MISSING_TYPE.MISSING] = ...,
        init: bool = True,
        repr: bool = True,
        hash: bool | None = None,
        compare: bool = True,
        metadata: Mapping[Any, Any] | None = None,
        kw_only: bool | Literal[_MISSING_TYPE.MISSING] = ...,
        doc: str | None = None,
    ) -> _T:
        """Return an object to identify dataclass fields.

        default is the default value of the field.  default_factory is a
        0-argument function called to initialize a field's value.  If init
        is true, the field will be a parameter to the class's __init__()
        function.  If repr is true, the field will be included in the
        object's repr().  If hash is true, the field will be included in the
        object's hash().  If compare is true, the field will be used in
        comparison functions.  metadata, if specified, must be a mapping
        which is stored but not otherwise examined by dataclass.  If kw_only
        is true, the field will become a keyword-only parameter to
        __init__().  doc is an optional docstring for this field.

        It is an error to specify both default and default_factory.
        """

    @overload
    def field(
        *,
        default: Literal[_MISSING_TYPE.MISSING] = ...,
        default_factory: Callable[[], _T],
        init: bool = True,
        repr: bool = True,
        hash: bool | None = None,
        compare: bool = True,
        metadata: Mapping[Any, Any] | None = None,
        kw_only: bool | Literal[_MISSING_TYPE.MISSING] = ...,
        doc: str | None = None,
    ) -> _T: ...
    @overload
    def field(
        *,
        default: Literal[_MISSING_TYPE.MISSING] = ...,
        default_factory: Literal[_MISSING_TYPE.MISSING] = ...,
        init: bool = True,
        repr: bool = True,
        hash: bool | None = None,
        compare: bool = True,
        metadata: Mapping[Any, Any] | None = None,
        kw_only: bool | Literal[_MISSING_TYPE.MISSING] = ...,
        doc: str | None = None,
    ) -> Any: ...

elif sys.version_info >= (3, 10):
    @overload  # `default` and `default_factory` are optional and mutually exclusive.
    def field(
        *,
        default: _T,
        default_factory: Literal[_MISSING_TYPE.MISSING] = ...,
        init: bool = True,
        repr: bool = True,
        hash: bool | None = None,
        compare: bool = True,
        metadata: Mapping[Any, Any] | None = None,
        kw_only: bool | Literal[_MISSING_TYPE.MISSING] = ...,
    ) -> _T:
        """Return an object to identify dataclass fields.

        default is the default value of the field.  default_factory is a
        0-argument function called to initialize a field's value.  If init
        is true, the field will be a parameter to the class's __init__()
        function.  If repr is true, the field will be included in the
        object's repr().  If hash is true, the field will be included in the
        object's hash().  If compare is true, the field will be used in
        comparison functions.  metadata, if specified, must be a mapping
        which is stored but not otherwise examined by dataclass.  If kw_only
        is true, the field will become a keyword-only parameter to
        __init__().

        It is an error to specify both default and default_factory.
        """

    @overload
    def field(
        *,
        default: Literal[_MISSING_TYPE.MISSING] = ...,
        default_factory: Callable[[], _T],
        init: bool = True,
        repr: bool = True,
        hash: bool | None = None,
        compare: bool = True,
        metadata: Mapping[Any, Any] | None = None,
        kw_only: bool | Literal[_MISSING_TYPE.MISSING] = ...,
    ) -> _T: ...
    @overload
    def field(
        *,
        default: Literal[_MISSING_TYPE.MISSING] = ...,
        default_factory: Literal[_MISSING_TYPE.MISSING] = ...,
        init: bool = True,
        repr: bool = True,
        hash: bool | None = None,
        compare: bool = True,
        metadata: Mapping[Any, Any] | None = None,
        kw_only: bool | Literal[_MISSING_TYPE.MISSING] = ...,
    ) -> Any: ...

else:
    @overload  # `default` and `default_factory` are optional and mutually exclusive.
    def field(
        *,
        default: _T,
        default_factory: Literal[_MISSING_TYPE.MISSING] = ...,
        init: bool = True,
        repr: bool = True,
        hash: bool | None = None,
        compare: bool = True,
        metadata: Mapping[Any, Any] | None = None,
    ) -> _T:
        """Return an object to identify dataclass fields.

        default is the default value of the field.  default_factory is a
        0-argument function called to initialize a field's value.  If init
        is True, the field will be a parameter to the class's __init__()
        function.  If repr is True, the field will be included in the
        object's repr().  If hash is True, the field will be included in
        the object's hash().  If compare is True, the field will be used
        in comparison functions.  metadata, if specified, must be a
        mapping which is stored but not otherwise examined by dataclass.

        It is an error to specify both default and default_factory.
        """

    @overload
    def field(
        *,
        default: Literal[_MISSING_TYPE.MISSING] = ...,
        default_factory: Callable[[], _T],
        init: bool = True,
        repr: bool = True,
        hash: bool | None = None,
        compare: bool = True,
        metadata: Mapping[Any, Any] | None = None,
    ) -> _T: ...
    @overload
    def field(
        *,
        default: Literal[_MISSING_TYPE.MISSING] = ...,
        default_factory: Literal[_MISSING_TYPE.MISSING] = ...,
        init: bool = True,
        repr: bool = True,
        hash: bool | None = None,
        compare: bool = True,
        metadata: Mapping[Any, Any] | None = None,
    ) -> Any: ...

def fields(class_or_instance: DataclassInstance | type[DataclassInstance]) -> tuple[Field[Any], ...]:
    """Return a tuple describing the fields of this dataclass.

    Accepts a dataclass or an instance of one. Tuple elements are of
    type Field.
    """

# HACK: `obj: Never` typing matches if object argument is using `Any` type.
@overload
def is_dataclass(obj: Never) -> TypeIs[DataclassInstance | type[DataclassInstance]]:  # type: ignore[narrowed-type-not-subtype]  # pyright: ignore[reportGeneralTypeIssues]
    """Returns True if obj is a dataclass or an instance of a
    dataclass.
    """

@overload
def is_dataclass(obj: type) -> TypeIs[type[DataclassInstance]]: ...
@overload
def is_dataclass(obj: object) -> TypeIs[DataclassInstance | type[DataclassInstance]]: ...

class FrozenInstanceError(AttributeError): ...

class InitVar(Generic[_T]):
    __slots__ = ("type",)
    type: Type[_T]
    def __init__(self, type: Type[_T]) -> None: ...
    @overload
    def __class_getitem__(cls, type: Type[_T]) -> InitVar[_T]: ...  # pyright: ignore[reportInvalidTypeForm]
    @overload
    def __class_getitem__(cls, type: Any) -> InitVar[Any]: ...  # pyright: ignore[reportInvalidTypeForm]

if sys.version_info >= (3, 14):
    def make_dataclass(
        cls_name: str,
        fields: Iterable[str | tuple[str, Any] | tuple[str, Any, Any]],
        *,
        bases: tuple[type, ...] = (),
        namespace: dict[str, Any] | None = None,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
        match_args: bool = True,
        kw_only: bool = False,
        slots: bool = False,
        weakref_slot: bool = False,
        module: str | None = None,
        decorator: _DataclassFactory = ...,
    ) -> type:
        """Return a new dynamically created dataclass.

        The dataclass name will be 'cls_name'.  'fields' is an iterable
        of either (name), (name, type) or (name, type, Field) objects. If type is
        omitted, use the string 'typing.Any'.  Field objects are created by
        the equivalent of calling 'field(name, type [, Field-info])'.::

          C = make_dataclass('C', ['x', ('y', int), ('z', int, field(init=False))], bases=(Base,))

        is equivalent to::

          @dataclass
          class C(Base):
              x: 'typing.Any'
              y: int
              z: int = field(init=False)

        For the bases and namespace parameters, see the builtin type() function.

        The parameters init, repr, eq, order, unsafe_hash, frozen, match_args, kw_only,
        slots, and weakref_slot are passed to dataclass().

        If module parameter is defined, the '__module__' attribute of the dataclass is
        set to that value.
        """

elif sys.version_info >= (3, 12):
    def make_dataclass(
        cls_name: str,
        fields: Iterable[str | tuple[str, Any] | tuple[str, Any, Any]],
        *,
        bases: tuple[type, ...] = (),
        namespace: dict[str, Any] | None = None,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
        match_args: bool = True,
        kw_only: bool = False,
        slots: bool = False,
        weakref_slot: bool = False,
        module: str | None = None,
    ) -> type:
        """Return a new dynamically created dataclass.

        The dataclass name will be 'cls_name'.  'fields' is an iterable
        of either (name), (name, type) or (name, type, Field) objects. If type is
        omitted, use the string 'typing.Any'.  Field objects are created by
        the equivalent of calling 'field(name, type [, Field-info])'.::

          C = make_dataclass('C', ['x', ('y', int), ('z', int, field(init=False))], bases=(Base,))

        is equivalent to::

          @dataclass
          class C(Base):
              x: 'typing.Any'
              y: int
              z: int = field(init=False)

        For the bases and namespace parameters, see the builtin type() function.

        The parameters init, repr, eq, order, unsafe_hash, frozen, match_args, kw_only,
        slots, and weakref_slot are passed to dataclass().

        If module parameter is defined, the '__module__' attribute of the dataclass is
        set to that value.
        """

elif sys.version_info >= (3, 11):
    def make_dataclass(
        cls_name: str,
        fields: Iterable[str | tuple[str, Any] | tuple[str, Any, Any]],
        *,
        bases: tuple[type, ...] = (),
        namespace: dict[str, Any] | None = None,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
        match_args: bool = True,
        kw_only: bool = False,
        slots: bool = False,
        weakref_slot: bool = False,
    ) -> type:
        """Return a new dynamically created dataclass.

        The dataclass name will be 'cls_name'.  'fields' is an iterable
        of either (name), (name, type) or (name, type, Field) objects. If type is
        omitted, use the string 'typing.Any'.  Field objects are created by
        the equivalent of calling 'field(name, type [, Field-info])'.::

          C = make_dataclass('C', ['x', ('y', int), ('z', int, field(init=False))], bases=(Base,))

        is equivalent to::

          @dataclass
          class C(Base):
              x: 'typing.Any'
              y: int
              z: int = field(init=False)

        For the bases and namespace parameters, see the builtin type() function.

        The parameters init, repr, eq, order, unsafe_hash, and frozen are passed to
        dataclass().
        """

elif sys.version_info >= (3, 10):
    def make_dataclass(
        cls_name: str,
        fields: Iterable[str | tuple[str, Any] | tuple[str, Any, Any]],
        *,
        bases: tuple[type, ...] = (),
        namespace: dict[str, Any] | None = None,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
        match_args: bool = True,
        kw_only: bool = False,
        slots: bool = False,
    ) -> type:
        """Return a new dynamically created dataclass.

        The dataclass name will be 'cls_name'.  'fields' is an iterable
        of either (name), (name, type) or (name, type, Field) objects. If type is
        omitted, use the string 'typing.Any'.  Field objects are created by
        the equivalent of calling 'field(name, type [, Field-info])'.

          C = make_dataclass('C', ['x', ('y', int), ('z', int, field(init=False))], bases=(Base,))

        is equivalent to:

          @dataclass
          class C(Base):
              x: 'typing.Any'
              y: int
              z: int = field(init=False)

        For the bases and namespace parameters, see the builtin type() function.

        The parameters init, repr, eq, order, unsafe_hash, and frozen are passed to
        dataclass().
        """

else:
    def make_dataclass(
        cls_name: str,
        fields: Iterable[str | tuple[str, Any] | tuple[str, Any, Any]],
        *,
        bases: tuple[type, ...] = (),
        namespace: dict[str, Any] | None = None,
        init: bool = True,
        repr: bool = True,
        eq: bool = True,
        order: bool = False,
        unsafe_hash: bool = False,
        frozen: bool = False,
    ) -> type:
        """Return a new dynamically created dataclass.

        The dataclass name will be 'cls_name'.  'fields' is an iterable
        of either (name), (name, type) or (name, type, Field) objects. If type is
        omitted, use the string 'typing.Any'.  Field objects are created by
        the equivalent of calling 'field(name, type [, Field-info])'.

          C = make_dataclass('C', ['x', ('y', int), ('z', int, field(init=False))], bases=(Base,))

        is equivalent to:

          @dataclass
          class C(Base):
              x: 'typing.Any'
              y: int
              z: int = field(init=False)

        For the bases and namespace parameters, see the builtin type() function.

        The parameters init, repr, eq, order, unsafe_hash, and frozen are passed to
        dataclass().
        """

def replace(obj: _DataclassT, /, **changes: Any) -> _DataclassT:
    """Return a new object replacing specified fields with new values.

    This is especially useful for frozen classes.  Example usage::

      @dataclass(frozen=True)
      class C:
          x: int
          y: int

      c = C(1, 2)
      c1 = replace(c, x=3)
      assert c1.x == 3 and c1.y == 2
    """
