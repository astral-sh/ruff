import _typeshed
import sys
import types
from _typeshed import SupportsKeysAndGetItem, Unused
from builtins import property as _builtins_property
from collections.abc import Callable, Iterable, Iterator, Mapping
from typing import Any, Final, Generic, Literal, TypeVar, overload
from typing_extensions import Self, TypeAlias, disjoint_base

__all__ = ["EnumMeta", "Enum", "IntEnum", "Flag", "IntFlag", "auto", "unique"]

if sys.version_info >= (3, 11):
    __all__ += [
        "CONFORM",
        "CONTINUOUS",
        "EJECT",
        "EnumCheck",
        "EnumType",
        "FlagBoundary",
        "KEEP",
        "NAMED_FLAGS",
        "ReprEnum",
        "STRICT",
        "StrEnum",
        "UNIQUE",
        "global_enum",
        "global_enum_repr",
        "global_flag_repr",
        "global_str",
        "member",
        "nonmember",
        "property",
        "verify",
        "pickle_by_enum_name",
        "pickle_by_global_name",
    ]

if sys.version_info >= (3, 13):
    __all__ += ["EnumDict"]

_EnumMemberT = TypeVar("_EnumMemberT")
_EnumerationT = TypeVar("_EnumerationT", bound=type[Enum])

# The following all work:
# >>> from enum import Enum
# >>> from string import ascii_lowercase
# >>> Enum('Foo', names='RED YELLOW GREEN')
# <enum 'Foo'>
# >>> Enum('Foo', names=[('RED', 1), ('YELLOW, 2)])
# <enum 'Foo'>
# >>> Enum('Foo', names=((x for x in (ascii_lowercase[i], i)) for i in range(5)))
# <enum 'Foo'>
# >>> Enum('Foo', names={'RED': 1, 'YELLOW': 2})
# <enum 'Foo'>
_EnumNames: TypeAlias = str | Iterable[str] | Iterable[Iterable[str | Any]] | Mapping[str, Any]
_Signature: TypeAlias = Any  # TODO: Unable to import Signature from inspect module

if sys.version_info >= (3, 11):
    class nonmember(Generic[_EnumMemberT]):
        """
        Protects item from becoming an Enum member during class creation.
        """

        value: _EnumMemberT
        def __init__(self, value: _EnumMemberT) -> None: ...

    class member(Generic[_EnumMemberT]):
        """
        Forces item to become an Enum member during class creation.
        """

        value: _EnumMemberT
        def __init__(self, value: _EnumMemberT) -> None: ...

class _EnumDict(dict[str, Any]):
    """
    Track enum member order and ensure member names are not reused.

    EnumType will use the names found in self._member_names as the
    enumeration member names.
    """

    if sys.version_info >= (3, 13):
        def __init__(self, cls_name: str | None = None) -> None: ...
    else:
        def __init__(self) -> None: ...

    def __setitem__(self, key: str, value: Any) -> None:
        """
        Changes anything not dundered or not a descriptor.

        If an enum member name is used twice, an error is raised; duplicate
        values are not checked for.

        Single underscore (sunder) names are reserved.
        """
    if sys.version_info >= (3, 11):
        # See comment above `typing.MutableMapping.update`
        # for why overloads are preferable to a Union here
        #
        # Unlike with MutableMapping.update(), the first argument is required,
        # hence the type: ignore
        @overload  # type: ignore[override]
        def update(self, members: SupportsKeysAndGetItem[str, Any], **more_members: Any) -> None: ...
        @overload
        def update(self, members: Iterable[tuple[str, Any]], **more_members: Any) -> None: ...
    if sys.version_info >= (3, 13):
        @property
        def member_names(self) -> list[str]: ...

if sys.version_info >= (3, 13):
    EnumDict = _EnumDict

# Structurally: Iterable[T], Reversible[T], Container[T] where T is the enum itself
class EnumMeta(type):
    """
    Metaclass for Enum
    """

    if sys.version_info >= (3, 11):
        def __new__(
            metacls: type[_typeshed.Self],
            cls: str,
            bases: tuple[type, ...],
            classdict: _EnumDict,
            *,
            boundary: FlagBoundary | None = None,
            _simple: bool = False,
            **kwds: Any,
        ) -> _typeshed.Self: ...
    else:
        def __new__(
            metacls: type[_typeshed.Self], cls: str, bases: tuple[type, ...], classdict: _EnumDict, **kwds: Any
        ) -> _typeshed.Self: ...

    @classmethod
    def __prepare__(metacls, cls: str, bases: tuple[type, ...], **kwds: Any) -> _EnumDict: ...  # type: ignore[override]
    def __iter__(self: type[_EnumMemberT]) -> Iterator[_EnumMemberT]:
        """
        Return members in definition order.
        """

    def __reversed__(self: type[_EnumMemberT]) -> Iterator[_EnumMemberT]:
        """
        Return members in reverse definition order.
        """
    if sys.version_info >= (3, 12):
        def __contains__(self: type[Any], value: object) -> bool:
            """Return True if `value` is in `cls`.

            `value` is in `cls` if:
            1) `value` is a member of `cls`, or
            2) `value` is the value of one of the `cls`'s members.
            3) `value` is a pseudo-member (flags)
            """
    elif sys.version_info >= (3, 11):
        def __contains__(self: type[Any], member: object) -> bool:
            """
            Return True if member is a member of this enum
            raises TypeError if member is not an enum member

            note: in 3.12 TypeError will no longer be raised, and True will also be
            returned if member is the value of a member in this enum
            """
    elif sys.version_info >= (3, 10):
        def __contains__(self: type[Any], obj: object) -> bool: ...
    else:
        def __contains__(self: type[Any], member: object) -> bool: ...

    def __getitem__(self: type[_EnumMemberT], name: str) -> _EnumMemberT:
        """
        Return the member matching `name`.
        """

    @_builtins_property
    def __members__(self: type[_EnumMemberT]) -> types.MappingProxyType[str, _EnumMemberT]:
        """
        Returns a mapping of member name->value.

        This mapping lists all enum members, including aliases. Note that this
        is a read-only view of the internal mapping.
        """

    def __len__(self) -> int:
        """
        Return the number of members (no aliases)
        """

    def __bool__(self) -> Literal[True]:
        """
        classes/types should always be True.
        """

    def __dir__(self) -> list[str]: ...

    # Overload 1: Value lookup on an already existing enum class (simple case)
    @overload
    def __call__(cls: type[_EnumMemberT], value: Any, names: None = None) -> _EnumMemberT:
        """
        Either returns an existing member, or creates a new enum class.

        This method is used both when an enum class is given a value to match
        to an enumeration member (i.e. Color(3)) and for the functional API
        (i.e. Color = Enum('Color', names='RED GREEN BLUE')).

        The value lookup branch is chosen if the enum is final.

        When used for the functional API:

        `value` will be the name of the new class.

        `names` should be either a string of white-space/comma delimited names
        (values will start at `start`), or an iterator/mapping of name, value pairs.

        `module` should be set to the module this class is being created in;
        if it is not set, an attempt to find that module will be made, but if
        it fails the class will not be picklable.

        `qualname` should be set to the actual location this class can be found
        at in its module; by default it is set to the global scope.  If this is
        not correct, unpickling will fail in some circumstances.

        `type`, if set, will be mixed in as the first base class.
        """
    # Overload 2: Functional API for constructing new enum classes.
    if sys.version_info >= (3, 11):
        @overload
        def __call__(
            cls,
            value: str,
            names: _EnumNames,
            *,
            module: str | None = None,
            qualname: str | None = None,
            type: type | None = None,
            start: int = 1,
            boundary: FlagBoundary | None = None,
        ) -> type[Enum]:
            """
            Either returns an existing member, or creates a new enum class.

            This method is used both when an enum class is given a value to match
            to an enumeration member (i.e. Color(3)) and for the functional API
            (i.e. Color = Enum('Color', names='RED GREEN BLUE')).

            The value lookup branch is chosen if the enum is final.

            When used for the functional API:

            `value` will be the name of the new class.

            `names` should be either a string of white-space/comma delimited names
            (values will start at `start`), or an iterator/mapping of name, value pairs.

            `module` should be set to the module this class is being created in;
            if it is not set, an attempt to find that module will be made, but if
            it fails the class will not be picklable.

            `qualname` should be set to the actual location this class can be found
            at in its module; by default it is set to the global scope.  If this is
            not correct, unpickling will fail in some circumstances.

            `type`, if set, will be mixed in as the first base class.
            """
    else:
        @overload
        def __call__(
            cls,
            value: str,
            names: _EnumNames,
            *,
            module: str | None = None,
            qualname: str | None = None,
            type: type | None = None,
            start: int = 1,
        ) -> type[Enum]:
            """
            Either returns an existing member, or creates a new enum class.

            This method is used both when an enum class is given a value to match
            to an enumeration member (i.e. Color(3)) and for the functional API
            (i.e. Color = Enum('Color', names='RED GREEN BLUE')).

            When used for the functional API:

            `value` will be the name of the new class.

            `names` should be either a string of white-space/comma delimited names
            (values will start at `start`), or an iterator/mapping of name, value pairs.

            `module` should be set to the module this class is being created in;
            if it is not set, an attempt to find that module will be made, but if
            it fails the class will not be picklable.

            `qualname` should be set to the actual location this class can be found
            at in its module; by default it is set to the global scope.  If this is
            not correct, unpickling will fail in some circumstances.

            `type`, if set, will be mixed in as the first base class.
            """
    # Overload 3 (py312+ only): Value lookup on an already existing enum class (complex case)
    #
    # >>> class Foo(enum.Enum):
    # ...     X = 1, 2, 3
    # >>> Foo(1, 2, 3)
    # <Foo.X: (1, 2, 3)>
    #
    if sys.version_info >= (3, 12):
        @overload
        def __call__(cls: type[_EnumMemberT], value: Any, *values: Any) -> _EnumMemberT:
            """
            Either returns an existing member, or creates a new enum class.

            This method is used both when an enum class is given a value to match
            to an enumeration member (i.e. Color(3)) and for the functional API
            (i.e. Color = Enum('Color', names='RED GREEN BLUE')).

            The value lookup branch is chosen if the enum is final.

            When used for the functional API:

            `value` will be the name of the new class.

            `names` should be either a string of white-space/comma delimited names
            (values will start at `start`), or an iterator/mapping of name, value pairs.

            `module` should be set to the module this class is being created in;
            if it is not set, an attempt to find that module will be made, but if
            it fails the class will not be picklable.

            `qualname` should be set to the actual location this class can be found
            at in its module; by default it is set to the global scope.  If this is
            not correct, unpickling will fail in some circumstances.

            `type`, if set, will be mixed in as the first base class.
            """
    if sys.version_info >= (3, 14):
        @property
        def __signature__(cls) -> _Signature: ...

    _member_names_: list[str]  # undocumented
    _member_map_: dict[str, Enum]  # undocumented
    _value2member_map_: dict[Any, Enum]  # undocumented

if sys.version_info >= (3, 11):
    # In 3.11 `EnumMeta` metaclass is renamed to `EnumType`, but old name also exists.
    EnumType = EnumMeta

    class property(types.DynamicClassAttribute):
        """
        This is a descriptor, used to define attributes that act differently
        when accessed through an enum member and through an enum class.
        Instance access is the same as property(), but access to an attribute
        through the enum class will instead look in the class' _member_map_ for
        a corresponding enum member.
        """

        def __set_name__(self, ownerclass: type[Enum], name: str) -> None: ...
        name: str
        clsname: str
        member: Enum | None

    _magic_enum_attr = property
else:
    _magic_enum_attr = types.DynamicClassAttribute

class Enum(metaclass=EnumMeta):
    """
    Create a collection of name/value pairs.

    Example enumeration:

    >>> class Color(Enum):
    ...     RED = 1
    ...     BLUE = 2
    ...     GREEN = 3

    Access them by:

    - attribute access:

      >>> Color.RED
      <Color.RED: 1>

    - value lookup:

      >>> Color(1)
      <Color.RED: 1>

    - name lookup:

      >>> Color['RED']
      <Color.RED: 1>

    Enumerations can be iterated over, and know how many members they have:

    >>> len(Color)
    3

    >>> list(Color)
    [<Color.RED: 1>, <Color.BLUE: 2>, <Color.GREEN: 3>]

    Methods can be added to enumerations, and members can have their own
    attributes -- see the documentation for details.
    """

    @_magic_enum_attr
    def name(self) -> str:
        """The name of the Enum member."""

    @_magic_enum_attr
    def value(self) -> Any:
        """The value of the Enum member."""
    _name_: str
    _value_: Any
    _ignore_: str | list[str]
    _order_: str
    __order__: str
    @classmethod
    def _missing_(cls, value: object) -> Any: ...
    @staticmethod
    def _generate_next_value_(name: str, start: int, count: int, last_values: list[Any]) -> Any:
        """
        Generate the next value when not given.

        name: the name of the member
        start: the initial start value or None
        count: the number of existing members
        last_values: the list of values assigned
        """
    # It's not true that `__new__` will accept any argument type,
    # so ideally we'd use `Any` to indicate that the argument type is inexpressible.
    # However, using `Any` causes too many false-positives for those using mypy's `--disallow-any-expr`
    # (see #7752, #2539, mypy/#5788),
    # and in practice using `object` here has the same effect as using `Any`.
    def __new__(cls, value: object) -> Self: ...
    def __dir__(self) -> list[str]:
        """
        Returns public methods and other interesting attributes.
        """

    def __hash__(self) -> int: ...
    def __format__(self, format_spec: str) -> str:
        """
        Returns format using actual value type unless __str__ has been overridden.
        """

    def __reduce_ex__(self, proto: Unused) -> tuple[Any, ...]: ...
    if sys.version_info >= (3, 11):
        def __copy__(self) -> Self: ...
        def __deepcopy__(self, memo: Any) -> Self: ...
    if sys.version_info >= (3, 12) and sys.version_info < (3, 14):
        @classmethod
        def __signature__(cls) -> str: ...
    if sys.version_info >= (3, 13):
        # Value may be any type, even in special enums. Enabling Enum parsing from
        # multiple value types
        def _add_value_alias_(self, value: Any) -> None: ...
        def _add_alias_(self, name: str) -> None: ...

if sys.version_info >= (3, 11):
    class ReprEnum(Enum):
        """
        Only changes the repr(), leaving str() and format() to the mixed-in type.
        """

if sys.version_info >= (3, 12):
    class IntEnum(int, ReprEnum):
        """
        Enum where members are also (and must be) ints
        """

        _value_: int
        @_magic_enum_attr
        def value(self) -> int:
            """The value of the Enum member."""

        def __new__(cls, value: int) -> Self: ...

else:
    if sys.version_info >= (3, 11):
        _IntEnumBase = ReprEnum
    else:
        _IntEnumBase = Enum

    @disjoint_base
    class IntEnum(int, _IntEnumBase):
        """
        Enum where members are also (and must be) ints
        """

        _value_: int
        @_magic_enum_attr
        def value(self) -> int:
            """The value of the Enum member."""

        def __new__(cls, value: int) -> Self: ...

def unique(enumeration: _EnumerationT) -> _EnumerationT:
    """
    Class decorator for enumerations ensuring unique member values.
    """

_auto_null: Any

class Flag(Enum):
    """
    Support for flags
    """

    _name_: str | None  # type: ignore[assignment]
    _value_: int
    @_magic_enum_attr
    def name(self) -> str | None:  # type: ignore[override]
        """The name of the Enum member."""

    @_magic_enum_attr
    def value(self) -> int:
        """The value of the Enum member."""

    def __contains__(self, other: Self) -> bool:
        """
        Returns True if self has at least the same flags set as other.
        """

    def __bool__(self) -> bool: ...
    def __or__(self, other: Self) -> Self: ...
    def __and__(self, other: Self) -> Self: ...
    def __xor__(self, other: Self) -> Self: ...
    def __invert__(self) -> Self: ...
    if sys.version_info >= (3, 11):
        def __iter__(self) -> Iterator[Self]:
            """
            Returns flags in definition order.
            """

        def __len__(self) -> int: ...
        __ror__ = __or__
        __rand__ = __and__
        __rxor__ = __xor__

if sys.version_info >= (3, 11):
    class StrEnum(str, ReprEnum):
        """
        Enum where members are also (and must be) strings
        """

        def __new__(cls, value: str) -> Self: ...
        _value_: str
        @_magic_enum_attr
        def value(self) -> str:
            """The value of the Enum member."""

        @staticmethod
        def _generate_next_value_(name: str, start: int, count: int, last_values: list[str]) -> str:
            """
            Return the lower-cased version of the member name.
            """

    class EnumCheck(StrEnum):
        """
        various conditions to check an enumeration for
        """

        CONTINUOUS = "no skipped integer values"
        NAMED_FLAGS = "multi-flag aliases may not contain unnamed flags"
        UNIQUE = "one name per value"

    CONTINUOUS: Final = EnumCheck.CONTINUOUS
    NAMED_FLAGS: Final = EnumCheck.NAMED_FLAGS
    UNIQUE: Final = EnumCheck.UNIQUE

    class verify:
        """
        Check an enumeration for various constraints. (see EnumCheck)
        """

        def __init__(self, *checks: EnumCheck) -> None: ...
        def __call__(self, enumeration: _EnumerationT) -> _EnumerationT: ...

    class FlagBoundary(StrEnum):
        """
        control how out of range values are handled
        "strict" -> error is raised             [default for Flag]
        "conform" -> extra bits are discarded
        "eject" -> lose flag status
        "keep" -> keep flag status and all bits [default for IntFlag]
        """

        STRICT = "strict"
        CONFORM = "conform"
        EJECT = "eject"
        KEEP = "keep"

    STRICT: Final = FlagBoundary.STRICT
    CONFORM: Final = FlagBoundary.CONFORM
    EJECT: Final = FlagBoundary.EJECT
    KEEP: Final = FlagBoundary.KEEP

    def global_str(self: Enum) -> str:
        """
        use enum_name instead of class.enum_name
        """

    def global_enum(cls: _EnumerationT, update_str: bool = False) -> _EnumerationT:
        """
        decorator that makes the repr() of an enum member reference its module
        instead of its class; also exports all members to the enum's module's
        global namespace
        """

    def global_enum_repr(self: Enum) -> str:
        """
        use module.enum_name instead of class.enum_name

        the module is the last module in case of a multi-module name
        """

    def global_flag_repr(self: Flag) -> str:
        """
        use module.flag_name instead of class.flag_name

        the module is the last module in case of a multi-module name
        """

    def show_flag_values(value: int) -> list[int]: ...

if sys.version_info >= (3, 12):
    # The body of the class is the same, but the base classes are different.
    class IntFlag(int, ReprEnum, Flag, boundary=KEEP):  # type: ignore[misc]  # complaints about incompatible bases
        """
        Support for integer-based Flags
        """

        def __new__(cls, value: int) -> Self: ...
        def __or__(self, other: int) -> Self: ...
        def __and__(self, other: int) -> Self: ...
        def __xor__(self, other: int) -> Self: ...
        def __invert__(self) -> Self: ...
        __ror__ = __or__
        __rand__ = __and__
        __rxor__ = __xor__

elif sys.version_info >= (3, 11):
    # The body of the class is the same, but the base classes are different.
    @disjoint_base
    class IntFlag(int, ReprEnum, Flag, boundary=KEEP):  # type: ignore[misc]  # complaints about incompatible bases
        """
        Support for integer-based Flags
        """

        def __new__(cls, value: int) -> Self: ...
        def __or__(self, other: int) -> Self: ...
        def __and__(self, other: int) -> Self: ...
        def __xor__(self, other: int) -> Self: ...
        def __invert__(self) -> Self: ...
        __ror__ = __or__
        __rand__ = __and__
        __rxor__ = __xor__

else:
    @disjoint_base
    class IntFlag(int, Flag):  # type: ignore[misc]  # complaints about incompatible bases
        """
        Support for integer-based Flags
        """

        def __new__(cls, value: int) -> Self: ...
        def __or__(self, other: int) -> Self: ...
        def __and__(self, other: int) -> Self: ...
        def __xor__(self, other: int) -> Self: ...
        def __invert__(self) -> Self: ...
        __ror__ = __or__
        __rand__ = __and__
        __rxor__ = __xor__

class auto:
    """
    Instances are replaced with an appropriate value in Enum class suites.
    """

    _value_: Any
    @_magic_enum_attr
    def value(self) -> Any: ...
    def __new__(cls) -> Self: ...

    # These don't exist, but auto is basically immediately replaced with
    # either an int or a str depending on the type of the enum. StrEnum's auto
    # shouldn't have these, but they're needed for int versions of auto (mostly the __or__).
    # Ideally type checkers would special case auto enough to handle this,
    # but until then this is a slightly inaccurate helping hand.
    def __or__(self, other: int | Self) -> Self:
        """Return self|value."""

    def __and__(self, other: int | Self) -> Self: ...
    def __xor__(self, other: int | Self) -> Self: ...
    __ror__ = __or__
    __rand__ = __and__
    __rxor__ = __xor__

if sys.version_info >= (3, 11):
    def pickle_by_global_name(self: Enum, proto: int) -> str: ...
    def pickle_by_enum_name(self: _EnumMemberT, proto: int) -> tuple[Callable[..., Any], tuple[type[_EnumMemberT], str]]: ...
