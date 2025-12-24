import abc
import enum
import sys
from _collections_abc import dict_items, dict_keys, dict_values
from _typeshed import AnnotationForm, IdentityFunction, Incomplete, Unused
from collections.abc import (
    AsyncGenerator as AsyncGenerator,
    AsyncIterable as AsyncIterable,
    AsyncIterator as AsyncIterator,
    Awaitable as Awaitable,
    Collection as Collection,
    Container as Container,
    Coroutine as Coroutine,
    Generator as Generator,
    Hashable as Hashable,
    ItemsView as ItemsView,
    Iterable as Iterable,
    Iterator as Iterator,
    KeysView as KeysView,
    Mapping as Mapping,
    MappingView as MappingView,
    MutableMapping as MutableMapping,
    MutableSequence as MutableSequence,
    MutableSet as MutableSet,
    Reversible as Reversible,
    Sequence as Sequence,
    Sized as Sized,
    ValuesView as ValuesView,
)
from contextlib import AbstractAsyncContextManager as AsyncContextManager, AbstractContextManager as ContextManager
from re import Match as Match, Pattern as Pattern
from types import GenericAlias, ModuleType
from typing import (  # noqa: Y022,Y037,Y038,Y039,UP035
    IO as IO,
    TYPE_CHECKING as TYPE_CHECKING,
    AbstractSet as AbstractSet,
    Any as Any,
    AnyStr as AnyStr,
    BinaryIO as BinaryIO,
    Callable as Callable,
    ChainMap as ChainMap,
    ClassVar as ClassVar,
    Counter as Counter,
    DefaultDict as DefaultDict,
    Deque as Deque,
    Dict as Dict,
    ForwardRef as ForwardRef,
    FrozenSet as FrozenSet,
    Generic as Generic,
    List as List,
    NoReturn as NoReturn,
    Optional as Optional,
    Set as Set,
    Text as Text,
    TextIO as TextIO,
    Tuple as Tuple,
    Type as Type,
    TypedDict as TypedDict,
    TypeVar as _TypeVar,
    Union as Union,
    _Alias,
    _SpecialForm,
    cast as cast,
    no_type_check as no_type_check,
    no_type_check_decorator as no_type_check_decorator,
    overload as overload,
    type_check_only,
)

if sys.version_info >= (3, 10):
    from types import UnionType

# Please keep order the same as at runtime.
__all__ = [
    # Super-special typing primitives.
    "Any",
    "ClassVar",
    "Concatenate",
    "Final",
    "LiteralString",
    "ParamSpec",
    "ParamSpecArgs",
    "ParamSpecKwargs",
    "Self",
    "Type",
    "TypeVar",
    "TypeVarTuple",
    "Unpack",
    # ABCs (from collections.abc).
    "Awaitable",
    "AsyncIterator",
    "AsyncIterable",
    "Coroutine",
    "AsyncGenerator",
    "AsyncContextManager",
    "Buffer",
    "ChainMap",
    # Concrete collection types.
    "ContextManager",
    "Counter",
    "Deque",
    "DefaultDict",
    "NamedTuple",
    "OrderedDict",
    "TypedDict",
    # Structural checks, a.k.a. protocols.
    "SupportsAbs",
    "SupportsBytes",
    "SupportsComplex",
    "SupportsFloat",
    "SupportsIndex",
    "SupportsInt",
    "SupportsRound",
    "Reader",
    "Writer",
    # One-off things.
    "Annotated",
    "assert_never",
    "assert_type",
    "clear_overloads",
    "dataclass_transform",
    "deprecated",
    "disjoint_base",
    "Doc",
    "evaluate_forward_ref",
    "get_overloads",
    "final",
    "Format",
    "get_annotations",
    "get_args",
    "get_origin",
    "get_original_bases",
    "get_protocol_members",
    "get_type_hints",
    "IntVar",
    "is_protocol",
    "is_typeddict",
    "Literal",
    "NewType",
    "overload",
    "override",
    "Protocol",
    "Sentinel",
    "reveal_type",
    "runtime",
    "runtime_checkable",
    "Text",
    "TypeAlias",
    "TypeAliasType",
    "TypeForm",
    "TypeGuard",
    "TypeIs",
    "TYPE_CHECKING",
    "type_repr",
    "Never",
    "NoReturn",
    "ReadOnly",
    "Required",
    "NotRequired",
    "NoDefault",
    "NoExtraItems",
    # Pure aliases, have always been in typing
    "AbstractSet",
    "AnyStr",
    "BinaryIO",
    "Callable",
    "Collection",
    "Container",
    "Dict",
    "ForwardRef",
    "FrozenSet",
    "Generator",
    "Generic",
    "Hashable",
    "IO",
    "ItemsView",
    "Iterable",
    "Iterator",
    "KeysView",
    "List",
    "Mapping",
    "MappingView",
    "Match",
    "MutableMapping",
    "MutableSequence",
    "MutableSet",
    "Optional",
    "Pattern",
    "Reversible",
    "Sequence",
    "Set",
    "Sized",
    "TextIO",
    "Tuple",
    "Union",
    "ValuesView",
    "cast",
    "no_type_check",
    "no_type_check_decorator",
    # Added dynamically
    "CapsuleType",
]

_T = _TypeVar("_T")
_F = _TypeVar("_F", bound=Callable[..., Any])
_TC = _TypeVar("_TC", bound=type[object])
_T_co = _TypeVar("_T_co", covariant=True)  # Any type covariant containers.
_T_contra = _TypeVar("_T_contra", contravariant=True)

# Do not import (and re-export) Protocol or runtime_checkable from
# typing module because type checkers need to be able to distinguish
# typing.Protocol and typing_extensions.Protocol so they can properly
# warn users about potential runtime exceptions when using typing.Protocol
# on older versions of Python.
Protocol: _SpecialForm

def runtime_checkable(cls: _TC) -> _TC:
    """Mark a protocol class as a runtime protocol.

    Such protocol can be used with isinstance() and issubclass().
    Raise TypeError if applied to a non-protocol class.
    This allows a simple-minded structural check very similar to
    one trick ponies in collections.abc such as Iterable.

    For example::

        @runtime_checkable
        class Closable(Protocol):
            def close(self): ...

        assert isinstance(open('/some/file'), Closable)

    Warning: this will check only the presence of the required methods,
    not their type signatures!
    """

# This alias for above is kept here for backwards compatibility.
runtime = runtime_checkable
Final: _SpecialForm

def final(f: _F) -> _F:
    """Decorator to indicate final methods and final classes.

    Use this decorator to indicate to type checkers that the decorated
    method cannot be overridden, and decorated class cannot be subclassed.

    For example::

        class Base:
            @final
            def done(self) -> None:
                ...
        class Sub(Base):
            def done(self) -> None:  # Error reported by type checker
                ...

        @final
        class Leaf:
            ...
        class Other(Leaf):  # Error reported by type checker
            ...

    There is no runtime checking of these properties. The decorator
    attempts to set the ``__final__`` attribute to ``True`` on the decorated
    object to allow runtime introspection.
    """

def disjoint_base(cls: _TC) -> _TC:
    """This decorator marks a class as a disjoint base.

    Child classes of a disjoint base cannot inherit from other disjoint bases that are
    not parent classes of the disjoint base.

    For example:

        @disjoint_base
        class Disjoint1: pass

        @disjoint_base
        class Disjoint2: pass

        class Disjoint3(Disjoint1, Disjoint2): pass  # Type checker error

    Type checkers can use knowledge of disjoint bases to detect unreachable code
    and determine when two types can overlap.

    See PEP 800.
    """

Literal: _SpecialForm

def IntVar(name: str) -> Any: ...  # returns a new TypeVar

# Internal mypy fallback type for all typed dicts (does not exist at runtime)
# N.B. Keep this mostly in sync with typing._TypedDict/mypy_extensions._TypedDict
@type_check_only
class _TypedDict(Mapping[str, object], metaclass=abc.ABCMeta):
    __required_keys__: ClassVar[frozenset[str]]
    __optional_keys__: ClassVar[frozenset[str]]
    __total__: ClassVar[bool]
    __orig_bases__: ClassVar[tuple[Any, ...]]
    # PEP 705
    __readonly_keys__: ClassVar[frozenset[str]]
    __mutable_keys__: ClassVar[frozenset[str]]
    # PEP 728
    __closed__: ClassVar[bool | None]
    __extra_items__: ClassVar[AnnotationForm]
    def copy(self) -> Self: ...
    # Using Never so that only calls using mypy plugin hook that specialize the signature
    # can go through.
    def setdefault(self, k: Never, default: object) -> object: ...
    # Mypy plugin hook for 'pop' expects that 'default' has a type variable type.
    def pop(self, k: Never, default: _T = ...) -> object: ...  # pyright: ignore[reportInvalidTypeVarUse]
    def update(self, m: Self, /) -> None: ...
    def items(self) -> dict_items[str, object]: ...
    def keys(self) -> dict_keys[str, object]: ...
    def values(self) -> dict_values[str, object]: ...
    def __delitem__(self, k: Never) -> None: ...
    @overload
    def __or__(self, value: Self, /) -> Self:
        """Return self|value."""

    @overload
    def __or__(self, value: dict[str, Any], /) -> dict[str, object]: ...
    @overload
    def __ror__(self, value: Self, /) -> Self:
        """Return value|self."""

    @overload
    def __ror__(self, value: dict[str, Any], /) -> dict[str, object]: ...
    # supposedly incompatible definitions of `__ior__` and `__or__`:
    # Since this module defines "Self" it is not recognized by Ruff as typing_extensions.Self
    def __ior__(self, value: Self, /) -> Self: ...  # type: ignore[misc]

OrderedDict = _Alias()

if sys.version_info >= (3, 13):
    from typing import get_type_hints as get_type_hints
else:
    def get_type_hints(
        obj: Any, globalns: dict[str, Any] | None = None, localns: Mapping[str, Any] | None = None, include_extras: bool = False
    ) -> dict[str, AnnotationForm]:
        """Return type hints for an object.

        This is often the same as obj.__annotations__, but it handles
        forward references encoded as string literals, adds Optional[t] if a
        default value equal to None is set and recursively replaces all
        'Annotated[T, ...]', 'Required[T]' or 'NotRequired[T]' with 'T'
        (unless 'include_extras=True').

        The argument may be a module, class, method, or function. The annotations
        are returned as a dictionary. For classes, annotations include also
        inherited members.

        TypeError is raised if the argument is not of a type that can contain
        annotations, and an empty dictionary is returned if no annotations are
        present.

        BEWARE -- the behavior of globalns and localns is counterintuitive
        (unless you are familiar with how eval() and exec() work).  The
        search order is locals first, then globals.

        - If no dict arguments are passed, an attempt is made to use the
          globals from obj (or the respective module's globals for classes),
          and these are also used as the locals.  If the object does not appear
          to have globals, an empty dictionary is used.

        - If one dict argument is passed, it is used for both globals and
          locals.

        - If two dict arguments are passed, they specify globals and
          locals, respectively.
        """

def get_args(tp: AnnotationForm) -> tuple[AnnotationForm, ...]:
    """Get type arguments with all substitutions performed.

    For unions, basic simplifications used by Union constructor are performed.

    Examples::

        >>> T = TypeVar('T')
        >>> assert get_args(Dict[str, int]) == (str, int)
        >>> assert get_args(int) == ()
        >>> assert get_args(Union[int, Union[T, int], str][int]) == (int, str)
        >>> assert get_args(Union[int, Tuple[T, int]][str]) == (int, Tuple[str, int])
        >>> assert get_args(Callable[[], T][int]) == ([], int)
    """

if sys.version_info >= (3, 10):
    @overload
    def get_origin(tp: UnionType) -> type[UnionType]:
        """Get the unsubscripted version of a type.

        This supports generic types, Callable, Tuple, Union, Literal, Final, ClassVar,
        Annotated, and others. Return None for unsupported types.

        Examples::

            >>> P = ParamSpec('P')
            >>> assert get_origin(Literal[42]) is Literal
            >>> assert get_origin(int) is None
            >>> assert get_origin(ClassVar[int]) is ClassVar
            >>> assert get_origin(Generic) is Generic
            >>> assert get_origin(Generic[T]) is Generic
            >>> assert get_origin(Union[T, int]) is Union
            >>> assert get_origin(List[Tuple[T, T]][int]) is list
            >>> assert get_origin(P.args) is P
        """

@overload
def get_origin(tp: GenericAlias) -> type:
    """Get the unsubscripted version of a type.

    This supports generic types, Callable, Tuple, Union, Literal, Final, ClassVar,
    Annotated, and others. Return None for unsupported types.

    Examples::

        >>> P = ParamSpec('P')
        >>> assert get_origin(Literal[42]) is Literal
        >>> assert get_origin(int) is None
        >>> assert get_origin(ClassVar[int]) is ClassVar
        >>> assert get_origin(Generic) is Generic
        >>> assert get_origin(Generic[T]) is Generic
        >>> assert get_origin(Union[T, int]) is Union
        >>> assert get_origin(List[Tuple[T, T]][int]) is list
        >>> assert get_origin(P.args) is P
    """

@overload
def get_origin(tp: ParamSpecArgs | ParamSpecKwargs) -> ParamSpec: ...
@overload
def get_origin(tp: AnnotationForm) -> AnnotationForm | None: ...

Annotated: _SpecialForm
_AnnotatedAlias: Any  # undocumented

# New and changed things in 3.10
if sys.version_info >= (3, 10):
    from typing import (
        Concatenate as Concatenate,
        ParamSpecArgs as ParamSpecArgs,
        ParamSpecKwargs as ParamSpecKwargs,
        TypeAlias as TypeAlias,
        TypeGuard as TypeGuard,
        is_typeddict as is_typeddict,
    )
else:
    @final
    class ParamSpecArgs:
        """The args for a ParamSpec object.

        Given a ParamSpec object P, P.args is an instance of ParamSpecArgs.

        ParamSpecArgs objects have a reference back to their ParamSpec:

        P.args.__origin__ is P

        This type is meant for runtime introspection and has no special meaning to
        static type checkers.
        """

        @property
        def __origin__(self) -> ParamSpec: ...
        def __init__(self, origin: ParamSpec) -> None: ...

    @final
    class ParamSpecKwargs:
        """The kwargs for a ParamSpec object.

        Given a ParamSpec object P, P.kwargs is an instance of ParamSpecKwargs.

        ParamSpecKwargs objects have a reference back to their ParamSpec:

        P.kwargs.__origin__ is P

        This type is meant for runtime introspection and has no special meaning to
        static type checkers.
        """

        @property
        def __origin__(self) -> ParamSpec: ...
        def __init__(self, origin: ParamSpec) -> None: ...

    Concatenate: _SpecialForm
    TypeAlias: _SpecialForm
    TypeGuard: _SpecialForm
    def is_typeddict(tp: object) -> bool:
        """Check if an annotation is a TypedDict class

        For example::
            class Film(TypedDict):
                title: str
                year: int

            is_typeddict(Film)  # => True
            is_typeddict(Union[list, str])  # => False
        """

# New and changed things in 3.11
if sys.version_info >= (3, 11):
    from typing import (
        LiteralString as LiteralString,
        NamedTuple as NamedTuple,
        Never as Never,
        NewType as NewType,
        NotRequired as NotRequired,
        Required as Required,
        Self as Self,
        Unpack as Unpack,
        assert_never as assert_never,
        assert_type as assert_type,
        clear_overloads as clear_overloads,
        dataclass_transform as dataclass_transform,
        get_overloads as get_overloads,
        reveal_type as reveal_type,
    )
else:
    Self: _SpecialForm
    Never: _SpecialForm
    def reveal_type(obj: _T, /) -> _T:
        """Reveal the inferred type of a variable.

        When a static type checker encounters a call to ``reveal_type()``,
        it will emit the inferred type of the argument::

            x: int = 1
            reveal_type(x)

        Running a static type checker (e.g., ``mypy``) on this example
        will produce output similar to 'Revealed type is "builtins.int"'.

        At runtime, the function prints the runtime type of the
        argument and returns it unchanged.

        """

    def assert_never(arg: Never, /) -> Never:
        """Assert to the type checker that a line of code is unreachable.

        Example::

            def int_or_str(arg: int | str) -> None:
                match arg:
                    case int():
                        print("It's an int")
                    case str():
                        print("It's a str")
                    case _:
                        assert_never(arg)

        If a type checker finds that a call to assert_never() is
        reachable, it will emit an error.

        At runtime, this throws an exception when called.

        """

    def assert_type(val: _T, typ: AnnotationForm, /) -> _T:
        """Assert (to the type checker) that the value is of the given type.

        When the type checker encounters a call to assert_type(), it
        emits an error if the value is not of the specified type::

            def greet(name: str) -> None:
                assert_type(name, str)  # ok
                assert_type(name, int)  # type checker error

        At runtime this returns the first argument unchanged and otherwise
        does nothing.
        """

    def clear_overloads() -> None:
        """Clear all overloads in the registry."""

    def get_overloads(func: Callable[..., object]) -> Sequence[Callable[..., object]]:
        """Return all defined overloads for *func* as a sequence."""
    Required: _SpecialForm
    NotRequired: _SpecialForm
    LiteralString: _SpecialForm
    Unpack: _SpecialForm

    def dataclass_transform(
        *,
        eq_default: bool = True,
        order_default: bool = False,
        kw_only_default: bool = False,
        frozen_default: bool = False,
        field_specifiers: tuple[type[Any] | Callable[..., Any], ...] = (),
        **kwargs: object,
    ) -> IdentityFunction:
        """Decorator that marks a function, class, or metaclass as providing
        dataclass-like behavior.

        Example:

            from typing_extensions import dataclass_transform

            _T = TypeVar("_T")

            # Used on a decorator function
            @dataclass_transform()
            def create_model(cls: type[_T]) -> type[_T]:
                ...
                return cls

            @create_model
            class CustomerModel:
                id: int
                name: str

            # Used on a base class
            @dataclass_transform()
            class ModelBase: ...

            class CustomerModel(ModelBase):
                id: int
                name: str

            # Used on a metaclass
            @dataclass_transform()
            class ModelMeta(type): ...

            class ModelBase(metaclass=ModelMeta): ...

            class CustomerModel(ModelBase):
                id: int
                name: str

        Each of the ``CustomerModel`` classes defined in this example will now
        behave similarly to a dataclass created with the ``@dataclasses.dataclass``
        decorator. For example, the type checker will synthesize an ``__init__``
        method.

        The arguments to this decorator can be used to customize this behavior:
        - ``eq_default`` indicates whether the ``eq`` parameter is assumed to be
          True or False if it is omitted by the caller.
        - ``order_default`` indicates whether the ``order`` parameter is
          assumed to be True or False if it is omitted by the caller.
        - ``kw_only_default`` indicates whether the ``kw_only`` parameter is
          assumed to be True or False if it is omitted by the caller.
        - ``frozen_default`` indicates whether the ``frozen`` parameter is
          assumed to be True or False if it is omitted by the caller.
        - ``field_specifiers`` specifies a static list of supported classes
          or functions that describe fields, similar to ``dataclasses.field()``.

        At runtime, this decorator records its arguments in the
        ``__dataclass_transform__`` attribute on the decorated object.

        See PEP 681 for details.

        """

    class NamedTuple(tuple[Any, ...]):
        """Typed version of namedtuple.

        Usage::

            class Employee(NamedTuple):
                name: str
                id: int

        This is equivalent to::

            Employee = collections.namedtuple('Employee', ['name', 'id'])

        The resulting class has an extra __annotations__ attribute, giving a
        dict that maps field names to types.  (The field names are also in
        the _fields attribute, which is part of the namedtuple API.)
        An alternative equivalent functional syntax is also accepted::

            Employee = NamedTuple('Employee', [('name', str), ('id', int)])
        """

        _field_defaults: ClassVar[dict[str, Any]]
        _fields: ClassVar[tuple[str, ...]]
        __orig_bases__: ClassVar[tuple[Any, ...]]
        @overload
        def __init__(self, typename: str, fields: Iterable[tuple[str, Any]] = ...) -> None: ...
        @overload
        def __init__(self, typename: str, fields: None = None, **kwargs: Any) -> None: ...
        @classmethod
        def _make(cls, iterable: Iterable[Any]) -> Self: ...
        def _asdict(self) -> dict[str, Any]: ...
        def _replace(self, **kwargs: Any) -> Self: ...

    class NewType:
        """NewType creates simple unique types with almost zero
        runtime overhead. NewType(name, tp) is considered a subtype of tp
        by static type checkers. At runtime, NewType(name, tp) returns
        a dummy callable that simply returns its argument. Usage::
            UserId = NewType('UserId', int)
            def name_by_id(user_id: UserId) -> str:
                ...
            UserId('user')          # Fails type check
            name_by_id(42)          # Fails type check
            name_by_id(UserId(42))  # OK
            num = UserId(5) + 1     # type: int
        """

        def __init__(self, name: str, tp: AnnotationForm) -> None: ...
        def __call__(self, obj: _T, /) -> _T: ...
        __supertype__: type | NewType
        __name__: str
        if sys.version_info >= (3, 10):
            def __or__(self, other: Any) -> _SpecialForm: ...
            def __ror__(self, other: Any) -> _SpecialForm: ...

if sys.version_info >= (3, 12):
    from collections.abc import Buffer as Buffer
    from types import get_original_bases as get_original_bases
    from typing import (
        SupportsAbs as SupportsAbs,
        SupportsBytes as SupportsBytes,
        SupportsComplex as SupportsComplex,
        SupportsFloat as SupportsFloat,
        SupportsIndex as SupportsIndex,
        SupportsInt as SupportsInt,
        SupportsRound as SupportsRound,
        override as override,
    )
else:
    def override(arg: _F, /) -> _F:
        """Indicate that a method is intended to override a method in a base class.

        Usage:

            class Base:
                def method(self) -> None:
                    pass

            class Child(Base):
                @override
                def method(self) -> None:
                    super().method()

        When this decorator is applied to a method, the type checker will
        validate that it overrides a method with the same name on a base class.
        This helps prevent bugs that may occur when a base class is changed
        without an equivalent change to a child class.

        There is no runtime checking of these properties. The decorator
        sets the ``__override__`` attribute to ``True`` on the decorated object
        to allow runtime introspection.

        See PEP 698 for details.

        """

    def get_original_bases(cls: type, /) -> tuple[Any, ...]:
        """Return the class's "original" bases prior to modification by `__mro_entries__`.

        Examples::

            from typing import TypeVar, Generic
            from typing_extensions import NamedTuple, TypedDict

            T = TypeVar("T")
            class Foo(Generic[T]): ...
            class Bar(Foo[int], float): ...
            class Baz(list[str]): ...
            Eggs = NamedTuple("Eggs", [("a", int), ("b", str)])
            Spam = TypedDict("Spam", {"a": int, "b": str})

            assert get_original_bases(Bar) == (Foo[int], float)
            assert get_original_bases(Baz) == (list[str],)
            assert get_original_bases(Eggs) == (NamedTuple,)
            assert get_original_bases(Spam) == (TypedDict,)
            assert get_original_bases(int) == (object,)
        """
    # mypy and pyright object to this being both ABC and Protocol.
    # At runtime it inherits from ABC and is not a Protocol, but it is on the
    # allowlist for use as a Protocol.
    @runtime_checkable
    class Buffer(Protocol, abc.ABC):  # type: ignore[misc]  # pyright: ignore[reportGeneralTypeIssues]
        """Base class for classes that implement the buffer protocol.

        The buffer protocol allows Python objects to expose a low-level
        memory buffer interface. Before Python 3.12, it is not possible
        to implement the buffer protocol in pure Python code, or even
        to check whether a class implements the buffer protocol. In
        Python 3.12 and higher, the ``__buffer__`` method allows access
        to the buffer protocol from Python code, and the
        ``collections.abc.Buffer`` ABC allows checking whether a class
        implements the buffer protocol.

        To indicate support for the buffer protocol in earlier versions,
        inherit from this ABC, either in a stub file or at runtime,
        or use ABC registration. This ABC provides no methods, because
        there is no Python-accessible methods shared by pre-3.12 buffer
        classes. It is useful primarily for static checks.

        """

        # Not actually a Protocol at runtime; see
        # https://github.com/python/typeshed/issues/10224 for why we're defining it this way
        def __buffer__(self, flags: int, /) -> memoryview: ...

    @runtime_checkable
    class SupportsInt(Protocol, metaclass=abc.ABCMeta):
        """An ABC with one abstract method __int__."""

        __slots__ = ()
        @abc.abstractmethod
        def __int__(self) -> int: ...

    @runtime_checkable
    class SupportsFloat(Protocol, metaclass=abc.ABCMeta):
        """An ABC with one abstract method __float__."""

        __slots__ = ()
        @abc.abstractmethod
        def __float__(self) -> float: ...

    @runtime_checkable
    class SupportsComplex(Protocol, metaclass=abc.ABCMeta):
        """An ABC with one abstract method __complex__."""

        __slots__ = ()
        @abc.abstractmethod
        def __complex__(self) -> complex: ...

    @runtime_checkable
    class SupportsBytes(Protocol, metaclass=abc.ABCMeta):
        """An ABC with one abstract method __bytes__."""

        __slots__ = ()
        @abc.abstractmethod
        def __bytes__(self) -> bytes: ...

    @runtime_checkable
    class SupportsIndex(Protocol, metaclass=abc.ABCMeta):
        __slots__ = ()
        @abc.abstractmethod
        def __index__(self) -> int: ...

    @runtime_checkable
    class SupportsAbs(Protocol[_T_co]):
        """
        An ABC with one abstract method __abs__ that is covariant in its return type.
        """

        __slots__ = ()
        @abc.abstractmethod
        def __abs__(self) -> _T_co: ...

    @runtime_checkable
    class SupportsRound(Protocol[_T_co]):
        """
        An ABC with one abstract method __round__ that is covariant in its return type.
        """

        __slots__ = ()
        @overload
        @abc.abstractmethod
        def __round__(self) -> int: ...
        @overload
        @abc.abstractmethod
        def __round__(self, ndigits: int, /) -> _T_co: ...

if sys.version_info >= (3, 14):
    from io import Reader as Reader, Writer as Writer
else:
    @runtime_checkable
    class Reader(Protocol[_T_co]):
        """Protocol for simple I/O reader instances.

        This protocol only supports blocking I/O.
        """

        __slots__ = ()
        @abc.abstractmethod
        def read(self, size: int = ..., /) -> _T_co:
            """Read data from the input stream and return it.

            If *size* is specified, at most *size* items (bytes/characters) will be
            read.
            """

    @runtime_checkable
    class Writer(Protocol[_T_contra]):
        """Protocol for simple I/O writer instances.

        This protocol only supports blocking I/O.
        """

        __slots__ = ()
        @abc.abstractmethod
        def write(self, data: _T_contra, /) -> int:
            """Write *data* to the output stream and return the number of items written."""

if sys.version_info >= (3, 13):
    from types import CapsuleType as CapsuleType
    from typing import (
        NoDefault as NoDefault,
        ParamSpec as ParamSpec,
        ReadOnly as ReadOnly,
        TypeIs as TypeIs,
        TypeVar as TypeVar,
        TypeVarTuple as TypeVarTuple,
        get_protocol_members as get_protocol_members,
        is_protocol as is_protocol,
    )
    from warnings import deprecated as deprecated
else:
    def is_protocol(tp: type, /) -> bool:
        """Return True if the given type is a Protocol.

        Example::

            >>> from typing_extensions import Protocol, is_protocol
            >>> class P(Protocol):
            ...     def a(self) -> str: ...
            ...     b: int
            >>> is_protocol(P)
            True
            >>> is_protocol(int)
            False
        """

    def get_protocol_members(tp: type, /) -> frozenset[str]:
        """Return the set of members defined in a Protocol.

        Example::

            >>> from typing_extensions import Protocol, get_protocol_members
            >>> class P(Protocol):
            ...     def a(self) -> str: ...
            ...     b: int
            >>> get_protocol_members(P)
            frozenset({'a', 'b'})

        Raise a TypeError for arguments that are not Protocols.
        """

    @final
    @type_check_only
    class _NoDefaultType: ...

    NoDefault: _NoDefaultType
    @final
    class CapsuleType:
        """Capsule objects let you wrap a C "void *" pointer in a Python
        object.  They're a way of passing data through the Python interpreter
        without creating your own custom type.

        Capsules are used for communication between extension modules.
        They provide a way for an extension module to export a C interface
        to other extension modules, so that extension modules can use the
        Python import mechanism to link to one another.
        """

    class deprecated:
        """Indicate that a class, function or overload is deprecated.

        When this decorator is applied to an object, the type checker
        will generate a diagnostic on usage of the deprecated object.

        Usage:

            @deprecated("Use B instead")
            class A:
                pass

            @deprecated("Use g instead")
            def f():
                pass

            @overload
            @deprecated("int support is deprecated")
            def g(x: int) -> int: ...
            @overload
            def g(x: str) -> int: ...

        The warning specified by *category* will be emitted at runtime
        on use of deprecated objects. For functions, that happens on calls;
        for classes, on instantiation and on creation of subclasses.
        If the *category* is ``None``, no warning is emitted at runtime.
        The *stacklevel* determines where the
        warning is emitted. If it is ``1`` (the default), the warning
        is emitted at the direct caller of the deprecated object; if it
        is higher, it is emitted further up the stack.
        Static type checker behavior is not affected by the *category*
        and *stacklevel* arguments.

        The deprecation message passed to the decorator is saved in the
        ``__deprecated__`` attribute on the decorated object.
        If applied to an overload, the decorator
        must be after the ``@overload`` decorator for the attribute to
        exist on the overload as returned by ``get_overloads()``.

        See PEP 702 for details.

        """

        message: LiteralString
        category: type[Warning] | None
        stacklevel: int
        def __init__(self, message: LiteralString, /, *, category: type[Warning] | None = ..., stacklevel: int = 1) -> None: ...
        def __call__(self, arg: _T, /) -> _T: ...

    @final
    class TypeVar:
        """Type variable."""

        @property
        def __name__(self) -> str: ...
        @property
        def __bound__(self) -> AnnotationForm | None: ...
        @property
        def __constraints__(self) -> tuple[AnnotationForm, ...]: ...
        @property
        def __covariant__(self) -> bool: ...
        @property
        def __contravariant__(self) -> bool: ...
        @property
        def __infer_variance__(self) -> bool: ...
        @property
        def __default__(self) -> AnnotationForm: ...
        def __init__(
            self,
            name: str,
            *constraints: AnnotationForm,
            bound: AnnotationForm | None = None,
            covariant: bool = False,
            contravariant: bool = False,
            default: AnnotationForm = ...,
            infer_variance: bool = False,
        ) -> None: ...
        def has_default(self) -> bool: ...
        def __typing_prepare_subst__(self, alias: Any, args: Any) -> tuple[Any, ...]: ...
        if sys.version_info >= (3, 10):
            def __or__(self, right: Any) -> _SpecialForm:
                """Return self|value."""

            def __ror__(self, left: Any) -> _SpecialForm:
                """Return value|self."""
        if sys.version_info >= (3, 11):
            def __typing_subst__(self, arg: Any) -> Any: ...

    @final
    class ParamSpec:
        """Parameter specification."""

        @property
        def __name__(self) -> str: ...
        @property
        def __bound__(self) -> AnnotationForm | None: ...
        @property
        def __covariant__(self) -> bool: ...
        @property
        def __contravariant__(self) -> bool: ...
        @property
        def __infer_variance__(self) -> bool: ...
        @property
        def __default__(self) -> AnnotationForm: ...
        def __init__(
            self,
            name: str,
            *,
            bound: None | AnnotationForm | str = None,
            contravariant: bool = False,
            covariant: bool = False,
            default: AnnotationForm = ...,
        ) -> None: ...
        @property
        def args(self) -> ParamSpecArgs: ...
        @property
        def kwargs(self) -> ParamSpecKwargs: ...
        def has_default(self) -> bool: ...
        def __typing_prepare_subst__(self, alias: Any, args: Any) -> tuple[Any, ...]: ...
        if sys.version_info >= (3, 10):
            def __or__(self, right: Any) -> _SpecialForm:
                """Return self|value."""

            def __ror__(self, left: Any) -> _SpecialForm:
                """Return value|self."""

    @final
    class TypeVarTuple:
        """Type variable tuple."""

        @property
        def __name__(self) -> str: ...
        @property
        def __default__(self) -> AnnotationForm: ...
        def __init__(self, name: str, *, default: AnnotationForm = ...) -> None: ...
        def __iter__(self) -> Any: ...  # Unpack[Self]
        def has_default(self) -> bool: ...
        def __typing_prepare_subst__(self, alias: Any, args: Any) -> tuple[Any, ...]: ...

    ReadOnly: _SpecialForm
    TypeIs: _SpecialForm

# TypeAliasType was added in Python 3.12, but had significant changes in 3.14.
if sys.version_info >= (3, 14):
    from typing import TypeAliasType as TypeAliasType
else:
    @final
    class TypeAliasType:
        """Create named, parameterized type aliases.

        This provides a backport of the new `type` statement in Python 3.12:

            type ListOrSet[T] = list[T] | set[T]

        is equivalent to:

            T = TypeVar("T")
            ListOrSet = TypeAliasType("ListOrSet", list[T] | set[T], type_params=(T,))

        The name ListOrSet can then be used as an alias for the type it refers to.

        The type_params argument should contain all the type parameters used
        in the value of the type alias. If the alias is not generic, this
        argument is omitted.

        Static type checkers should only support type aliases declared using
        TypeAliasType that follow these rules:

        - The first argument (the name) must be a string literal.
        - The TypeAliasType instance must be immediately assigned to a variable
          of the same name. (For example, 'X = TypeAliasType("Y", int)' is invalid,
          as is 'X, Y = TypeAliasType("X", int), TypeAliasType("Y", int)').

        """

        def __init__(
            self, name: str, value: AnnotationForm, *, type_params: tuple[TypeVar | ParamSpec | TypeVarTuple, ...] = ()
        ) -> None: ...
        @property
        def __value__(self) -> AnnotationForm: ...
        @property
        def __type_params__(self) -> tuple[TypeVar | ParamSpec | TypeVarTuple, ...]: ...
        @property
        # `__parameters__` can include special forms if a `TypeVarTuple` was
        # passed as a `type_params` element to the constructor method.
        def __parameters__(self) -> tuple[TypeVar | ParamSpec | AnnotationForm, ...]: ...
        @property
        def __name__(self) -> str: ...
        # It's writable on types, but not on instances of TypeAliasType.
        @property
        def __module__(self) -> str | None:  # type: ignore[override]
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
        # Returns typing._GenericAlias, which isn't stubbed.
        def __getitem__(self, parameters: Incomplete | tuple[Incomplete, ...]) -> AnnotationForm: ...
        def __init_subclass__(cls, *args: Unused, **kwargs: Unused) -> NoReturn: ...
        if sys.version_info >= (3, 10):
            def __or__(self, right: Any, /) -> _SpecialForm: ...
            def __ror__(self, left: Any, /) -> _SpecialForm: ...

# PEP 727
class Doc:
    """Define the documentation of a type annotation using ``Annotated``, to be
     used in class attributes, function and method parameters, return values,
     and variables.

    The value should be a positional-only string literal to allow static tools
    like editors and documentation generators to use it.

    This complements docstrings.

    The string value passed is available in the attribute ``documentation``.

    Example::

        >>> from typing_extensions import Annotated, Doc
        >>> def hi(to: Annotated[str, Doc("Who to say hi to")]) -> None: ...
    """

    documentation: str
    def __init__(self, documentation: str, /) -> None: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...

# PEP 728
@type_check_only
class _NoExtraItemsType: ...

NoExtraItems: _NoExtraItemsType

# PEP 747
TypeForm: _SpecialForm

# PEP 649/749
if sys.version_info >= (3, 14):
    from typing import evaluate_forward_ref as evaluate_forward_ref

    from annotationlib import Format as Format, get_annotations as get_annotations, type_repr as type_repr
else:
    class Format(enum.IntEnum):
        """An enumeration."""

        VALUE = 1
        VALUE_WITH_FAKE_GLOBALS = 2
        FORWARDREF = 3
        STRING = 4

    @overload
    def get_annotations(
        obj: Any,  # any object with __annotations__ or __annotate__
        *,
        globals: Mapping[str, Any] | None = None,  # value types depend on the key
        locals: Mapping[str, Any] | None = None,  # value types depend on the key
        eval_str: bool = False,
        format: Literal[Format.STRING],
    ) -> dict[str, str]:
        """Compute the annotations dict for an object.

        obj may be a callable, class, or module.
        Passing in an object of any other type raises TypeError.

        Returns a dict.  get_annotations() returns a new dict every time
        it's called; calling it twice on the same object will return two
        different but equivalent dicts.

        This is a backport of `inspect.get_annotations`, which has been
        in the standard library since Python 3.10. See the standard library
        documentation for more:

            https://docs.python.org/3/library/inspect.html#inspect.get_annotations

        This backport adds the *format* argument introduced by PEP 649. The
        three formats supported are:
        * VALUE: the annotations are returned as-is. This is the default and
          it is compatible with the behavior on previous Python versions.
        * FORWARDREF: return annotations as-is if possible, but replace any
          undefined names with ForwardRef objects. The implementation proposed by
          PEP 649 relies on language changes that cannot be backported; the
          typing-extensions implementation simply returns the same result as VALUE.
        * STRING: return annotations as strings, in a format close to the original
          source. Again, this behavior cannot be replicated directly in a backport.
          As an approximation, typing-extensions retrieves the annotations under
          VALUE semantics and then stringifies them.

        The purpose of this backport is to allow users who would like to use
        FORWARDREF or STRING semantics once PEP 649 is implemented, but who also
        want to support earlier Python versions, to simply write:

            typing_extensions.get_annotations(obj, format=Format.FORWARDREF)

        """

    @overload
    def get_annotations(
        obj: Any,  # any object with __annotations__ or __annotate__
        *,
        globals: Mapping[str, Any] | None = None,  # value types depend on the key
        locals: Mapping[str, Any] | None = None,  # value types depend on the key
        eval_str: bool = False,
        format: Literal[Format.FORWARDREF],
    ) -> dict[str, AnnotationForm | ForwardRef]: ...
    @overload
    def get_annotations(
        obj: Any,  # any object with __annotations__ or __annotate__
        *,
        globals: Mapping[str, Any] | None = None,  # value types depend on the key
        locals: Mapping[str, Any] | None = None,  # value types depend on the key
        eval_str: bool = False,
        format: Format = Format.VALUE,  # noqa: Y011
    ) -> dict[str, AnnotationForm]: ...
    @overload
    def evaluate_forward_ref(
        forward_ref: ForwardRef,
        *,
        owner: Callable[..., object] | type[object] | ModuleType | None = None,  # any callable, class, or module
        globals: Mapping[str, Any] | None = None,  # value types depend on the key
        locals: Mapping[str, Any] | None = None,  # value types depend on the key
        type_params: Iterable[TypeVar | ParamSpec | TypeVarTuple] | None = None,
        format: Literal[Format.STRING],
        _recursive_guard: Container[str] = ...,
    ) -> str:
        """Evaluate a forward reference as a type hint.

        This is similar to calling the ForwardRef.evaluate() method,
        but unlike that method, evaluate_forward_ref() also:

        * Recursively evaluates forward references nested within the type hint.
        * Rejects certain objects that are not valid type hints.
        * Replaces type hints that evaluate to None with types.NoneType.
        * Supports the *FORWARDREF* and *STRING* formats.

        *forward_ref* must be an instance of ForwardRef. *owner*, if given,
        should be the object that holds the annotations that the forward reference
        derived from, such as a module, class object, or function. It is used to
        infer the namespaces to use for looking up names. *globals* and *locals*
        can also be explicitly given to provide the global and local namespaces.
        *type_params* is a tuple of type parameters that are in scope when
        evaluating the forward reference. This parameter must be provided (though
        it may be an empty tuple) if *owner* is not given and the forward reference
        does not already have an owner set. *format* specifies the format of the
        annotation and is a member of the annotationlib.Format enum.

        """

    @overload
    def evaluate_forward_ref(
        forward_ref: ForwardRef,
        *,
        owner: Callable[..., object] | type[object] | ModuleType | None = None,  # any callable, class, or module
        globals: Mapping[str, Any] | None = None,  # value types depend on the key
        locals: Mapping[str, Any] | None = None,  # value types depend on the key
        type_params: Iterable[TypeVar | ParamSpec | TypeVarTuple] | None = None,
        format: Literal[Format.FORWARDREF],
        _recursive_guard: Container[str] = ...,
    ) -> AnnotationForm | ForwardRef: ...
    @overload
    def evaluate_forward_ref(
        forward_ref: ForwardRef,
        *,
        owner: Callable[..., object] | type[object] | ModuleType | None = None,  # any callable, class, or module
        globals: Mapping[str, Any] | None = None,  # value types depend on the key
        locals: Mapping[str, Any] | None = None,  # value types depend on the key
        type_params: Iterable[TypeVar | ParamSpec | TypeVarTuple] | None = None,
        format: Format | None = None,
        _recursive_guard: Container[str] = ...,
    ) -> AnnotationForm: ...
    def type_repr(value: object) -> str:
        """Convert a Python value to a format suitable for use with the STRING format.

        This is intended as a helper for tools that support the STRING format but do
        not have access to the code that originally produced the annotations. It uses
        repr() for most objects.

        """

# PEP 661
class Sentinel:
    """Create a unique sentinel object.

    *name* should be the name of the variable to which the return value shall be assigned.

    *repr*, if supplied, will be used for the repr of the sentinel object.
    If not provided, "<name>" will be used.
    """

    def __init__(self, name: str, repr: str | None = None) -> None: ...
    if sys.version_info >= (3, 14):
        def __or__(self, other: Any) -> UnionType: ...  # other can be any type form legal for unions
        def __ror__(self, other: Any) -> UnionType: ...  # other can be any type form legal for unions
    elif sys.version_info >= (3, 10):
        def __or__(self, other: Any) -> _SpecialForm: ...  # other can be any type form legal for unions
        def __ror__(self, other: Any) -> _SpecialForm: ...  # other can be any type form legal for unions
