"""
The typing module: Support for gradual typing as defined by PEP 484 and subsequent PEPs.

Among other things, the module includes the following:
* Generic, Protocol, and internal machinery to support generic aliases.
  All subscripted types like X[int], Union[int, str] are generic aliases.
* Various "special forms" that have unique meanings in type annotations:
  NoReturn, Never, ClassVar, Self, Concatenate, Unpack, and others.
* Classes whose instances can be type arguments to generic classes and functions:
  TypeVar, ParamSpec, TypeVarTuple.
* Public helper functions: get_type_hints, overload, cast, final, and others.
* Several protocols to support duck-typing:
  SupportsFloat, SupportsIndex, SupportsAbs, and others.
* Special types: NewType, NamedTuple, TypedDict.
* Deprecated aliases for builtin types and collections.abc ABCs.

Any name not present in __all__ is an implementation detail
that may be changed without notice. Use at your own risk!
"""

# Since this module defines "overload" it is not recognized by Ruff as typing.overload
# TODO: The collections import is required, otherwise mypy crashes.
# https://github.com/python/mypy/issues/16744
import collections  # noqa: F401  # pyright: ignore[reportUnusedImport]
import sys
import typing_extensions
from _collections_abc import dict_items, dict_keys, dict_values
from _typeshed import IdentityFunction, ReadableBuffer, SupportsGetItem, SupportsGetItemViewable, SupportsKeysAndGetItem, Viewable
from abc import ABCMeta, abstractmethod
from re import Match as Match, Pattern as Pattern
from types import (
    BuiltinFunctionType,
    CodeType,
    FunctionType,
    GenericAlias,
    MethodDescriptorType,
    MethodType,
    MethodWrapperType,
    ModuleType,
    TracebackType,
    WrapperDescriptorType,
)
from typing_extensions import Never as _Never, ParamSpec as _ParamSpec, deprecated

if sys.version_info >= (3, 14):
    from _typeshed import EvaluateFunc

    from annotationlib import Format

if sys.version_info >= (3, 10):
    from types import UnionType

__all__ = [
    "AbstractSet",
    "Annotated",
    "Any",
    "AnyStr",
    "AsyncContextManager",
    "AsyncGenerator",
    "AsyncIterable",
    "AsyncIterator",
    "Awaitable",
    "BinaryIO",
    "ByteString",
    "Callable",
    "ChainMap",
    "ClassVar",
    "Collection",
    "Container",
    "ContextManager",
    "Coroutine",
    "Counter",
    "DefaultDict",
    "Deque",
    "Dict",
    "Final",
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
    "Literal",
    "Mapping",
    "MappingView",
    "Match",
    "MutableMapping",
    "MutableSequence",
    "MutableSet",
    "NamedTuple",
    "NewType",
    "NoReturn",
    "Optional",
    "OrderedDict",
    "Pattern",
    "Protocol",
    "Reversible",
    "Sequence",
    "Set",
    "Sized",
    "SupportsAbs",
    "SupportsBytes",
    "SupportsComplex",
    "SupportsFloat",
    "SupportsIndex",
    "SupportsInt",
    "SupportsRound",
    "Text",
    "TextIO",
    "Tuple",
    "Type",
    "TypeVar",
    "TypedDict",
    "Union",
    "ValuesView",
    "TYPE_CHECKING",
    "cast",
    "final",
    "get_args",
    "get_origin",
    "get_type_hints",
    "no_type_check",
    "no_type_check_decorator",
    "overload",
    "runtime_checkable",
]

if sys.version_info >= (3, 14):
    __all__ += ["evaluate_forward_ref"]

if sys.version_info >= (3, 10):
    __all__ += ["Concatenate", "ParamSpec", "ParamSpecArgs", "ParamSpecKwargs", "TypeAlias", "TypeGuard", "is_typeddict"]

if sys.version_info >= (3, 11):
    __all__ += [
        "LiteralString",
        "Never",
        "NotRequired",
        "Required",
        "Self",
        "TypeVarTuple",
        "Unpack",
        "assert_never",
        "assert_type",
        "clear_overloads",
        "dataclass_transform",
        "get_overloads",
        "reveal_type",
    ]

if sys.version_info >= (3, 12):
    __all__ += ["TypeAliasType", "override"]

if sys.version_info >= (3, 13):
    __all__ += ["get_protocol_members", "is_protocol", "NoDefault", "TypeIs", "ReadOnly"]

# We can't use this name here because it leads to issues with mypy, likely
# due to an import cycle. Below instead we use Any with a comment.
# from _typeshed import AnnotationForm

class Any:
    """Special type indicating an unconstrained type.

    - Any is compatible with every type.
    - Any assumed to have all methods.
    - All values assumed to be instances of Any.

    Note that all the above statements are true from the point of view of
    static type checkers. At runtime, Any should not be used with instance
    checks.
    """

class _Final:
    """Mixin to prohibit subclassing."""

    __slots__ = ("__weakref__",)

def final(f: _T) -> _T:
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

@final
class TypeVar:
    """Type variable.

    The preferred way to construct a type variable is via the dedicated
    syntax for generic functions, classes, and type aliases::

        class Sequence[T]:  # T is a TypeVar
            ...

    This syntax can also be used to create bound and constrained type
    variables::

        # S is a TypeVar bound to str
        class StrSequence[S: str]:
            ...

        # A is a TypeVar constrained to str or bytes
        class StrOrBytesSequence[A: (str, bytes)]:
            ...

    Type variables can also have defaults:

        class IntDefault[T = int]:
            ...

    However, if desired, reusable type variables can also be constructed
    manually, like so::

       T = TypeVar('T')  # Can be anything
       S = TypeVar('S', bound=str)  # Can be any subtype of str
       A = TypeVar('A', str, bytes)  # Must be exactly str or bytes
       D = TypeVar('D', default=int)  # Defaults to int

    Type variables exist primarily for the benefit of static type
    checkers.  They serve as the parameters for generic types as well
    as for generic function and type alias definitions.

    The variance of type variables is inferred by type checkers when they
    are created through the type parameter syntax and when
    ``infer_variance=True`` is passed. Manually created type variables may
    be explicitly marked covariant or contravariant by passing
    ``covariant=True`` or ``contravariant=True``. By default, manually
    created type variables are invariant. See PEP 484 and PEP 695 for more
    details.
    """

    @property
    def __name__(self) -> str: ...
    @property
    def __bound__(self) -> Any | None: ...  # AnnotationForm
    @property
    def __constraints__(self) -> tuple[Any, ...]: ...  # AnnotationForm
    @property
    def __covariant__(self) -> bool: ...
    @property
    def __contravariant__(self) -> bool: ...
    if sys.version_info >= (3, 12):
        @property
        def __infer_variance__(self) -> bool: ...
    if sys.version_info >= (3, 13):
        @property
        def __default__(self) -> Any: ...  # AnnotationForm
    if sys.version_info >= (3, 13):
        def __new__(
            cls,
            name: str,
            *constraints: Any,  # AnnotationForm
            bound: Any | None = None,  # AnnotationForm
            contravariant: bool = False,
            covariant: bool = False,
            infer_variance: bool = False,
            default: Any = ...,  # AnnotationForm
        ) -> Self: ...
    elif sys.version_info >= (3, 12):
        def __new__(
            cls,
            name: str,
            *constraints: Any,  # AnnotationForm
            bound: Any | None = None,  # AnnotationForm
            covariant: bool = False,
            contravariant: bool = False,
            infer_variance: bool = False,
        ) -> Self: ...
    elif sys.version_info >= (3, 11):
        def __new__(
            cls,
            name: str,
            *constraints: Any,  # AnnotationForm
            bound: Any | None = None,  # AnnotationForm
            covariant: bool = False,
            contravariant: bool = False,
        ) -> Self: ...
    else:
        def __init__(
            self,
            name: str,
            *constraints: Any,  # AnnotationForm
            bound: Any | None = None,  # AnnotationForm
            covariant: bool = False,
            contravariant: bool = False,
        ) -> None: ...
    if sys.version_info >= (3, 10):
        def __or__(self, right: Any, /) -> _SpecialForm:  # AnnotationForm
            """Return self|value."""

        def __ror__(self, left: Any, /) -> _SpecialForm:  # AnnotationForm
            """Return value|self."""
    if sys.version_info >= (3, 11):
        def __typing_subst__(self, arg: Any, /) -> Any: ...
    if sys.version_info >= (3, 13):
        def __typing_prepare_subst__(self, alias: Any, args: Any, /) -> tuple[Any, ...]: ...
        def has_default(self) -> bool: ...
    if sys.version_info >= (3, 14):
        @property
        def evaluate_bound(self) -> EvaluateFunc | None: ...
        @property
        def evaluate_constraints(self) -> EvaluateFunc | None: ...
        @property
        def evaluate_default(self) -> EvaluateFunc | None: ...

# N.B. Keep this definition in sync with typing_extensions._SpecialForm
@final
class _SpecialForm(_Final):
    __slots__ = ("_name", "__doc__", "_getitem")
    def __getitem__(self, parameters: Any) -> object: ...
    if sys.version_info >= (3, 10):
        def __or__(self, other: Any) -> _SpecialForm: ...
        def __ror__(self, other: Any) -> _SpecialForm: ...

Union: _SpecialForm
"""Represent a union type

E.g. for int | str
"""

Protocol: _SpecialForm
"""Base class for protocol classes.

Protocol classes are defined as::

    class Proto(Protocol):
        def meth(self) -> int:
            ...

Such classes are primarily used with static type checkers that recognize
structural subtyping (static duck-typing).

For example::

    class C:
        def meth(self) -> int:
            return 0

    def func(x: Proto) -> int:
        return x.meth()

    func(C())  # Passes static type check

See PEP 544 for details. Protocol classes decorated with
@typing.runtime_checkable act as simple-minded runtime protocols that check
only the presence of given attributes, ignoring their type signatures.
Protocol classes can be generic, they are defined as::

    class GenProto[T](Protocol):
        def meth(self) -> T:
            ...
"""

Callable: _SpecialForm
"""Deprecated alias to collections.abc.Callable.

Callable[[int], str] signifies a function that takes a single
parameter of type int and returns a str.

The subscription syntax must always be used with exactly two
values: the argument list and the return type.
The argument list must be a list of types, a ParamSpec,
Concatenate or ellipsis. The return type must be a single type.

There is no syntax to indicate optional or keyword arguments;
such function types are rarely used as callback types.
"""

Type: _SpecialForm
"""Deprecated alias to builtins.type.

builtins.type or typing.Type can be used to annotate class objects.
For example, suppose we have the following classes::

    class User: ...  # Abstract base for User classes
    class BasicUser(User): ...
    class ProUser(User): ...
    class TeamUser(User): ...

And a function that takes a class argument that's a subclass of
User and returns an instance of the corresponding class::

    def new_user[U](user_class: Type[U]) -> U:
        user = user_class()
        # (Here we could write the user object to a database)
        return user

    joe = new_user(BasicUser)

At this point the type checker knows that joe has type BasicUser.
"""

NoReturn: _SpecialForm
"""Special type indicating functions that never return.

Example::

    from typing import NoReturn

    def stop() -> NoReturn:
        raise Exception('no way')

NoReturn can also be used as a bottom type, a type that
has no values. Starting in Python 3.11, the Never type should
be used for this concept instead. Type checkers should treat the two
equivalently.
"""

ClassVar: _SpecialForm
"""Special type construct to mark class variables.

An annotation wrapped in ClassVar indicates that a given
attribute is intended to be used as a class variable and
should not be set on instances of that class.

Usage::

    class Starship:
        stats: ClassVar[dict[str, int]] = {} # class variable
        damage: int = 10                     # instance variable

ClassVar accepts only types and cannot be further subscribed.

Note that ClassVar is not a class itself, and should not
be used with isinstance() or issubclass().
"""

Optional: _SpecialForm
"""Optional[X] is equivalent to Union[X, None]."""

Tuple: _SpecialForm
"""Deprecated alias to builtins.tuple.

Tuple[X, Y] is the cross-product type of X and Y.

Example: Tuple[T1, T2] is a tuple of two elements corresponding
to type variables T1 and T2.  Tuple[int, float, str] is a tuple
of an int, a float and a string.

To specify a variable-length tuple of homogeneous type, use Tuple[T, ...].
"""

Final: _SpecialForm
"""Special typing construct to indicate final names to type checkers.

A final name cannot be re-assigned or overridden in a subclass.

For example::

    MAX_SIZE: Final = 9000
    MAX_SIZE += 1  # Error reported by type checker

    class Connection:
        TIMEOUT: Final[int] = 10

    class FastConnector(Connection):
        TIMEOUT = 1  # Error reported by type checker

There is no runtime checking of these properties.
"""

Literal: _SpecialForm
"""Special typing form to define literal types (a.k.a. value types).

This form can be used to indicate to type checkers that the corresponding
variable or function parameter has a value equivalent to the provided
literal (or one of several literals)::

    def validate_simple(data: Any) -> Literal[True]:  # always returns True
        ...

    MODE = Literal['r', 'rb', 'w', 'wb']
    def open_helper(file: str, mode: MODE) -> str:
        ...

    open_helper('/some/path', 'r')  # Passes type check
    open_helper('/other/path', 'typo')  # Error in type checker

Literal[...] cannot be subclassed. At runtime, an arbitrary value
is allowed as type argument to Literal[...], but type checkers may
impose restrictions.
"""

TypedDict: _SpecialForm
"""A simple typed namespace. At runtime it is equivalent to a plain dict.

TypedDict creates a dictionary type such that a type checker will expect all
instances to have a certain set of keys, where each key is
associated with a value of a consistent type. This expectation
is not checked at runtime.

Usage::

    >>> class Point2D(TypedDict):
    ...     x: int
    ...     y: int
    ...     label: str
    ...
    >>> a: Point2D = {'x': 1, 'y': 2, 'label': 'good'}  # OK
    >>> b: Point2D = {'z': 3, 'label': 'bad'}           # Fails type check
    >>> Point2D(x=1, y=2, label='first') == dict(x=1, y=2, label='first')
    True

The type info can be accessed via the Point2D.__annotations__ dict, and
the Point2D.__required_keys__ and Point2D.__optional_keys__ frozensets.
TypedDict supports an additional equivalent form::

    Point2D = TypedDict('Point2D', {'x': int, 'y': int, 'label': str})

By default, all keys must be present in a TypedDict. It is possible
to override this by specifying totality::

    class Point2D(TypedDict, total=False):
        x: int
        y: int

This means that a Point2D TypedDict can have any of the keys omitted. A type
checker is only expected to support a literal False or True as the value of
the total argument. True is the default, and makes all items defined in the
class body be required.

The Required and NotRequired special forms can also be used to mark
individual keys as being required or not required::

    class Point2D(TypedDict):
        x: int               # the "x" key must always be present (Required is the default)
        y: NotRequired[int]  # the "y" key can be omitted

See PEP 655 for more details on Required and NotRequired.

The ReadOnly special form can be used
to mark individual keys as immutable for type checkers::

    class DatabaseUser(TypedDict):
        id: ReadOnly[int]  # the "id" key must not be modified
        username: str      # the "username" key can be changed

"""

if sys.version_info >= (3, 11):
    Self: _SpecialForm
    """Used to spell the type of "self" in classes.

    Example::

        from typing import Self

        class Foo:
            def return_self(self) -> Self:
                ...
                return self

    This is especially useful for:
        - classmethods that are used as alternative constructors
        - annotating an `__enter__` method which returns self
    """

    Never: _SpecialForm
    """The bottom type, a type that has no members.

    This can be used to define a function that should never be
    called, or a function that never returns::

        from typing import Never

        def never_call_me(arg: Never) -> None:
            pass

        def int_or_str(arg: int | str) -> None:
            never_call_me(arg)  # type checker error
            match arg:
                case int():
                    print("It's an int")
                case str():
                    print("It's a str")
                case _:
                    never_call_me(arg)  # OK, arg is of type Never
    """

    Unpack: _SpecialForm
    """Type unpack operator.

    The type unpack operator takes the child types from some container type,
    such as `tuple[int, str]` or a `TypeVarTuple`, and 'pulls them out'.

    For example::

        # For some generic class `Foo`:
        Foo[Unpack[tuple[int, str]]]  # Equivalent to Foo[int, str]

        Ts = TypeVarTuple('Ts')
        # Specifies that `Bar` is generic in an arbitrary number of types.
        # (Think of `Ts` as a tuple of an arbitrary number of individual
        #  `TypeVar`s, which the `Unpack` is 'pulling out' directly into the
        #  `Generic[]`.)
        class Bar(Generic[Unpack[Ts]]): ...
        Bar[int]  # Valid
        Bar[int, str]  # Also valid

    From Python 3.11, this can also be done using the `*` operator::

        Foo[*tuple[int, str]]
        class Bar(Generic[*Ts]): ...

    And from Python 3.12, it can be done using built-in syntax for generics::

        Foo[*tuple[int, str]]
        class Bar[*Ts]: ...

    The operator can also be used along with a `TypedDict` to annotate
    `**kwargs` in a function signature::

        class Movie(TypedDict):
            name: str
            year: int

        # This function expects two keyword arguments - *name* of type `str` and
        # *year* of type `int`.
        def foo(**kwargs: Unpack[Movie]): ...

    Note that there is only some runtime checking of this operator. Not
    everything the runtime allows may be accepted by static type checkers.

    For more information, see PEPs 646 and 692.
    """

    Required: _SpecialForm
    """Special typing construct to mark a TypedDict key as required.

    This is mainly useful for total=False TypedDicts.

    For example::

        class Movie(TypedDict, total=False):
            title: Required[str]
            year: int

        m = Movie(
            title='The Matrix',  # typechecker error if key is omitted
            year=1999,
        )

    There is no runtime checking that a required key is actually provided
    when instantiating a related TypedDict.
    """

    NotRequired: _SpecialForm
    """Special typing construct to mark a TypedDict key as potentially missing.

    For example::

        class Movie(TypedDict):
            title: str
            year: NotRequired[int]

        m = Movie(
            title='The Matrix',  # typechecker error if key is omitted
            year=1999,
        )
    """

    LiteralString: _SpecialForm
    """Represents an arbitrary literal string.

    Example::

        from typing import LiteralString

        def run_query(sql: LiteralString) -> None:
            ...

        def caller(arbitrary_string: str, literal_string: LiteralString) -> None:
            run_query("SELECT * FROM students")  # OK
            run_query(literal_string)  # OK
            run_query("SELECT * FROM " + literal_string)  # OK
            run_query(arbitrary_string)  # type checker error
            run_query(  # type checker error
                f"SELECT * FROM students WHERE name = {arbitrary_string}"
            )

    Only string literals and other LiteralStrings are compatible
    with LiteralString. This provides a tool to help prevent
    security issues such as SQL injection.
    """

    @final
    class TypeVarTuple:
        """Type variable tuple. A specialized form of type variable that enables
        variadic generics.

        The preferred way to construct a type variable tuple is via the
        dedicated syntax for generic functions, classes, and type aliases,
        where a single '*' indicates a type variable tuple::

            def move_first_element_to_last[T, *Ts](tup: tuple[T, *Ts]) -> tuple[*Ts, T]:
                return (*tup[1:], tup[0])

        Type variables tuples can have default values:

            type AliasWithDefault[*Ts = (str, int)] = tuple[*Ts]

        For compatibility with Python 3.11 and earlier, TypeVarTuple objects
        can also be created as follows::

            Ts = TypeVarTuple('Ts')  # Can be given any name
            DefaultTs = TypeVarTuple('Ts', default=(str, int))

        Just as a TypeVar (type variable) is a placeholder for a single type,
        a TypeVarTuple is a placeholder for an *arbitrary* number of types. For
        example, if we define a generic class using a TypeVarTuple::

            class C[*Ts]: ...

        Then we can parameterize that class with an arbitrary number of type
        arguments::

            C[int]       # Fine
            C[int, str]  # Also fine
            C[()]        # Even this is fine

        For more details, see PEP 646.

        Note that only TypeVarTuples defined in the global scope can be
        pickled.
        """

        @property
        def __name__(self) -> str: ...
        if sys.version_info >= (3, 13):
            @property
            def __default__(self) -> Any:  # AnnotationForm
                """The default value for this TypeVarTuple."""

            def has_default(self) -> bool: ...
        if sys.version_info >= (3, 13):
            def __new__(cls, name: str, *, default: Any = ...) -> Self: ...  # AnnotationForm
        elif sys.version_info >= (3, 12):
            def __new__(cls, name: str) -> Self: ...
        else:
            def __init__(self, name: str) -> None: ...

        def __iter__(self) -> Any:
            """Implement iter(self)."""

        def __typing_subst__(self, arg: Never, /) -> Never: ...
        def __typing_prepare_subst__(self, alias: Any, args: Any, /) -> tuple[Any, ...]: ...
        if sys.version_info >= (3, 14):
            @property
            def evaluate_default(self) -> EvaluateFunc | None: ...

if sys.version_info >= (3, 10):
    @final
    class ParamSpecArgs:
        """The args for a ParamSpec object.

        Given a ParamSpec object P, P.args is an instance of ParamSpecArgs.

        ParamSpecArgs objects have a reference back to their ParamSpec::

            >>> P = ParamSpec("P")
            >>> P.args.__origin__ is P
            True

        This type is meant for runtime introspection and has no special meaning
        to static type checkers.
        """

        @property
        def __origin__(self) -> ParamSpec: ...
        if sys.version_info >= (3, 12):
            def __new__(cls, origin: ParamSpec) -> Self: ...
        else:
            def __init__(self, origin: ParamSpec) -> None: ...

        def __eq__(self, other: object, /) -> bool: ...
        __hash__: ClassVar[None]  # type: ignore[assignment]

    @final
    class ParamSpecKwargs:
        """The kwargs for a ParamSpec object.

        Given a ParamSpec object P, P.kwargs is an instance of ParamSpecKwargs.

        ParamSpecKwargs objects have a reference back to their ParamSpec::

            >>> P = ParamSpec("P")
            >>> P.kwargs.__origin__ is P
            True

        This type is meant for runtime introspection and has no special meaning
        to static type checkers.
        """

        @property
        def __origin__(self) -> ParamSpec: ...
        if sys.version_info >= (3, 12):
            def __new__(cls, origin: ParamSpec) -> Self: ...
        else:
            def __init__(self, origin: ParamSpec) -> None: ...

        def __eq__(self, other: object, /) -> bool: ...
        __hash__: ClassVar[None]  # type: ignore[assignment]

    @final
    class ParamSpec:
        """Parameter specification variable.

        The preferred way to construct a parameter specification is via the
        dedicated syntax for generic functions, classes, and type aliases,
        where the use of '**' creates a parameter specification::

            type IntFunc[**P] = Callable[P, int]

        The following syntax creates a parameter specification that defaults
        to a callable accepting two positional-only arguments of types int
        and str:

            type IntFuncDefault[**P = (int, str)] = Callable[P, int]

        For compatibility with Python 3.11 and earlier, ParamSpec objects
        can also be created as follows::

            P = ParamSpec('P')
            DefaultP = ParamSpec('DefaultP', default=(int, str))

        Parameter specification variables exist primarily for the benefit of
        static type checkers.  They are used to forward the parameter types of
        one callable to another callable, a pattern commonly found in
        higher-order functions and decorators.  They are only valid when used
        in ``Concatenate``, or as the first argument to ``Callable``, or as
        parameters for user-defined Generics. See class Generic for more
        information on generic types.

        An example for annotating a decorator::

            def add_logging[**P, T](f: Callable[P, T]) -> Callable[P, T]:
                '''A type-safe decorator to add logging to a function.'''
                def inner(*args: P.args, **kwargs: P.kwargs) -> T:
                    logging.info(f'{f.__name__} was called')
                    return f(*args, **kwargs)
                return inner

            @add_logging
            def add_two(x: float, y: float) -> float:
                '''Add two numbers together.'''
                return x + y

        Parameter specification variables can be introspected. e.g.::

            >>> P = ParamSpec("P")
            >>> P.__name__
            'P'

        Note that only parameter specification variables defined in the global
        scope can be pickled.
        """

        @property
        def __name__(self) -> str: ...
        @property
        def __bound__(self) -> Any | None: ...  # AnnotationForm
        @property
        def __covariant__(self) -> bool: ...
        @property
        def __contravariant__(self) -> bool: ...
        if sys.version_info >= (3, 12):
            @property
            def __infer_variance__(self) -> bool: ...
        if sys.version_info >= (3, 13):
            @property
            def __default__(self) -> Any:  # AnnotationForm
                """The default value for this ParamSpec."""
        if sys.version_info >= (3, 13):
            def __new__(
                cls,
                name: str,
                *,
                bound: Any | None = None,  # AnnotationForm
                contravariant: bool = False,
                covariant: bool = False,
                infer_variance: bool = False,
                default: Any = ...,  # AnnotationForm
            ) -> Self: ...
        elif sys.version_info >= (3, 12):
            def __new__(
                cls,
                name: str,
                *,
                bound: Any | None = None,  # AnnotationForm
                contravariant: bool = False,
                covariant: bool = False,
                infer_variance: bool = False,
            ) -> Self: ...
        elif sys.version_info >= (3, 11):
            def __new__(
                cls,
                name: str,
                *,
                bound: Any | None = None,  # AnnotationForm
                contravariant: bool = False,
                covariant: bool = False,
            ) -> Self: ...
        else:
            def __init__(
                self,
                name: str,
                *,
                bound: Any | None = None,  # AnnotationForm
                contravariant: bool = False,
                covariant: bool = False,
            ) -> None: ...

        @property
        def args(self) -> ParamSpecArgs:
            """Represents positional arguments."""

        @property
        def kwargs(self) -> ParamSpecKwargs:
            """Represents keyword arguments."""
        if sys.version_info >= (3, 11):
            def __typing_subst__(self, arg: Any, /) -> Any: ...
            def __typing_prepare_subst__(self, alias: Any, args: Any, /) -> tuple[Any, ...]: ...

        def __or__(self, right: Any, /) -> _SpecialForm:
            """Return self|value."""

        def __ror__(self, left: Any, /) -> _SpecialForm:
            """Return value|self."""
        if sys.version_info >= (3, 13):
            def has_default(self) -> bool: ...
        if sys.version_info >= (3, 14):
            @property
            def evaluate_default(self) -> EvaluateFunc | None: ...

    Concatenate: _SpecialForm
    """Special form for annotating higher-order functions.

    ``Concatenate`` can be used in conjunction with ``ParamSpec`` and
    ``Callable`` to represent a higher-order function which adds, removes or
    transforms the parameters of a callable.

    For example::

        Callable[Concatenate[int, P], int]

    See PEP 612 for detailed information.
    """

    TypeAlias: _SpecialForm
    """Special form for marking type aliases.

    Use TypeAlias to indicate that an assignment should
    be recognized as a proper type alias definition by type
    checkers.

    For example::

        Predicate: TypeAlias = Callable[..., bool]

    It's invalid when used anywhere except as in the example above.
    """

    TypeGuard: _SpecialForm
    """Special typing construct for marking user-defined type predicate functions.

    ``TypeGuard`` can be used to annotate the return type of a user-defined
    type predicate function.  ``TypeGuard`` only accepts a single type argument.
    At runtime, functions marked this way should return a boolean.

    ``TypeGuard`` aims to benefit *type narrowing* -- a technique used by static
    type checkers to determine a more precise type of an expression within a
    program's code flow.  Usually type narrowing is done by analyzing
    conditional code flow and applying the narrowing to a block of code.  The
    conditional expression here is sometimes referred to as a "type predicate".

    Sometimes it would be convenient to use a user-defined boolean function
    as a type predicate.  Such a function should use ``TypeGuard[...]`` or
    ``TypeIs[...]`` as its return type to alert static type checkers to
    this intention. ``TypeGuard`` should be used over ``TypeIs`` when narrowing
    from an incompatible type (e.g., ``list[object]`` to ``list[int]``) or when
    the function does not return ``True`` for all instances of the narrowed type.

    Using  ``-> TypeGuard[NarrowedType]`` tells the static type checker that
    for a given function:

    1. The return value is a boolean.
    2. If the return value is ``True``, the type of its argument
       is ``NarrowedType``.

    For example::

         def is_str_list(val: list[object]) -> TypeGuard[list[str]]:
             '''Determines whether all objects in the list are strings'''
             return all(isinstance(x, str) for x in val)

         def func1(val: list[object]):
             if is_str_list(val):
                 # Type of ``val`` is narrowed to ``list[str]``.
                 print(" ".join(val))
             else:
                 # Type of ``val`` remains as ``list[object]``.
                 print("Not a list of strings!")

    Strict type narrowing is not enforced -- ``TypeB`` need not be a narrower
    form of ``TypeA`` (it can even be a wider form) and this may lead to
    type-unsafe results.  The main reason is to allow for things like
    narrowing ``list[object]`` to ``list[str]`` even though the latter is not
    a subtype of the former, since ``list`` is invariant.  The responsibility of
    writing type-safe type predicates is left to the user.

    ``TypeGuard`` also works with type variables.  For more information, see
    PEP 647 (User-Defined Type Guards).
    """

    class NewType:
        """NewType creates simple unique types with almost zero runtime overhead.

        NewType(name, tp) is considered a subtype of tp
        by static type checkers. At runtime, NewType(name, tp) returns
        a dummy callable that simply returns its argument.

        Usage::

            UserId = NewType('UserId', int)

            def name_by_id(user_id: UserId) -> str:
                ...

            UserId('user')          # Fails type check

            name_by_id(42)          # Fails type check
            name_by_id(UserId(42))  # OK

            num = UserId(5) + 1     # type: int
        """

        def __init__(self, name: str, tp: Any) -> None: ...  # AnnotationForm
        if sys.version_info >= (3, 11):
            @staticmethod
            def __call__(x: _T, /) -> _T: ...
        else:
            def __call__(self, x: _T) -> _T: ...

        def __or__(self, other: Any) -> _SpecialForm: ...
        def __ror__(self, other: Any) -> _SpecialForm: ...
        __supertype__: type | NewType
        __name__: str

else:
    def NewType(name: str, tp: Any) -> Any:
        """NewType creates simple unique types with almost zero
        runtime overhead. NewType(name, tp) is considered a subtype of tp
        by static type checkers. At runtime, NewType(name, tp) returns
        a dummy function that simply returns its argument. Usage::

            UserId = NewType('UserId', int)

            def name_by_id(user_id: UserId) -> str:
                ...

            UserId('user')          # Fails type check

            name_by_id(42)          # Fails type check
            name_by_id(UserId(42))  # OK

            num = UserId(5) + 1     # type: int
        """

_F = TypeVar("_F", bound=Callable[..., Any])
_P = _ParamSpec("_P")
_T = TypeVar("_T")

_FT = TypeVar("_FT", bound=Callable[..., Any] | type)

# These type variables are used by the container types.
_S = TypeVar("_S")
_KT = TypeVar("_KT")  # Key type.
_VT = TypeVar("_VT")  # Value type.
_T_co = TypeVar("_T_co", covariant=True)  # Any type covariant containers.
_KT_co = TypeVar("_KT_co", covariant=True)  # Key type covariant containers.
_VT_co = TypeVar("_VT_co", covariant=True)  # Value type covariant containers.
_TC = TypeVar("_TC", bound=type[object])

def overload(func: _F) -> _F:
    """Decorator for overloaded functions/methods.

    In a stub file, place two or more stub definitions for the same
    function in a row, each decorated with @overload.

    For example::

        @overload
        def utf8(value: None) -> None: ...
        @overload
        def utf8(value: bytes) -> bytes: ...
        @overload
        def utf8(value: str) -> bytes: ...

    In a non-stub file (i.e. a regular .py file), do the same but
    follow it with an implementation.  The implementation should *not*
    be decorated with @overload::

        @overload
        def utf8(value: None) -> None: ...
        @overload
        def utf8(value: bytes) -> bytes: ...
        @overload
        def utf8(value: str) -> bytes: ...
        def utf8(value):
            ...  # implementation goes here

    The overloads for a function can be retrieved at runtime using the
    get_overloads() function.
    """

def no_type_check(arg: _F) -> _F:
    """Decorator to indicate that annotations are not type hints.

    The argument must be a class or function; if it is a class, it
    applies recursively to all methods and classes defined in that class
    (but not to methods defined in its superclasses or subclasses).

    This mutates the function(s) or class(es) in place.
    """

if sys.version_info >= (3, 13):
    @deprecated("Deprecated since Python 3.13; removed in Python 3.15.")
    def no_type_check_decorator(decorator: Callable[_P, _T]) -> Callable[_P, _T]:
        """Decorator to give another decorator the @no_type_check effect.

        This wraps the decorator with something that wraps the decorated
        function in @no_type_check.
        """

else:
    def no_type_check_decorator(decorator: Callable[_P, _T]) -> Callable[_P, _T]:
        """Decorator to give another decorator the @no_type_check effect.

        This wraps the decorator with something that wraps the decorated
        function in @no_type_check.
        """

# This itself is only available during type checking
def type_check_only(func_or_cls: _FT) -> _FT: ...

# Type aliases and type constructors

@type_check_only
class _Alias:
    # Class for defining generic aliases for library types.
    def __getitem__(self, typeargs: Any) -> Any: ...

List = _Alias()
"""A generic version of list."""

Dict = _Alias()
"""A generic version of dict."""

DefaultDict = _Alias()
"""A generic version of collections.defaultdict."""

Set = _Alias()
"""A generic version of set."""

FrozenSet = _Alias()
"""A generic version of frozenset."""

Counter = _Alias()
"""A generic version of collections.Counter."""

Deque = _Alias()
"""A generic version of collections.deque."""

ChainMap = _Alias()
"""A generic version of collections.ChainMap."""

OrderedDict = _Alias()
"""A generic version of collections.OrderedDict."""

Annotated: _SpecialForm
"""Add context-specific metadata to a type.

Example: Annotated[int, runtime_check.Unsigned] indicates to the
hypothetical runtime_check module that this type is an unsigned int.
Every other consumer of this type can ignore this metadata and treat
this type as int.

The first argument to Annotated must be a valid type.

Details:

- It's an error to call `Annotated` with less than two arguments.
- Access the metadata via the ``__metadata__`` attribute::

    assert Annotated[int, '$'].__metadata__ == ('$',)

- Nested Annotated types are flattened::

    assert Annotated[Annotated[T, Ann1, Ann2], Ann3] == Annotated[T, Ann1, Ann2, Ann3]

- Instantiating an annotated type is equivalent to instantiating the
underlying type::

    assert Annotated[C, Ann1](5) == C(5)

- Annotated can be used as a generic type alias::

    type Optimized[T] = Annotated[T, runtime.Optimize()]
    # type checker will treat Optimized[int]
    # as equivalent to Annotated[int, runtime.Optimize()]

    type OptimizedList[T] = Annotated[list[T], runtime.Optimize()]
    # type checker will treat OptimizedList[int]
    # as equivalent to Annotated[list[int], runtime.Optimize()]

- Annotated cannot be used with an unpacked TypeVarTuple::

    type Variadic[*Ts] = Annotated[*Ts, Ann1]  # NOT valid

  This would be equivalent to::

    Annotated[T1, T2, T3, ..., Ann1]

  where T1, T2 etc. are TypeVars, which would be invalid, because
  only one type should be passed to Annotated.
"""

# Predefined type variables.
AnyStr = TypeVar("AnyStr", str, bytes)  # noqa: Y001

@type_check_only
class _Generic:
    if sys.version_info < (3, 12):
        __slots__ = ()

    if sys.version_info >= (3, 10):
        @classmethod
        def __class_getitem__(cls, args: TypeVar | ParamSpec | tuple[TypeVar | ParamSpec, ...]) -> _Final: ...
    else:
        @classmethod
        def __class_getitem__(cls, args: TypeVar | tuple[TypeVar, ...]) -> _Final: ...

Generic: type[_Generic]
"""Abstract base class for generic types.

On Python 3.12 and newer, generic classes implicitly inherit from
Generic when they declare a parameter list after the class's name::

    class Mapping[KT, VT]:
        def __getitem__(self, key: KT) -> VT:
            ...
        # Etc.

On older versions of Python, however, generic classes have to
explicitly inherit from Generic.

After a class has been declared to be generic, it can then be used as
follows::

    def lookup_name[KT, VT](mapping: Mapping[KT, VT], key: KT, default: VT) -> VT:
        try:
            return mapping[key]
        except KeyError:
            return default
"""

class _ProtocolMeta(ABCMeta):
    if sys.version_info >= (3, 12):
        def __init__(cls, *args: Any, **kwargs: Any) -> None: ...

# Abstract base classes.

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

@runtime_checkable
class SupportsInt(Protocol, metaclass=ABCMeta):
    """An ABC with one abstract method __int__."""

    __slots__ = ()
    @abstractmethod
    def __int__(self) -> int: ...

@runtime_checkable
class SupportsFloat(Protocol, metaclass=ABCMeta):
    """An ABC with one abstract method __float__."""

    __slots__ = ()
    @abstractmethod
    def __float__(self) -> float: ...

@runtime_checkable
class SupportsComplex(Protocol, metaclass=ABCMeta):
    """An ABC with one abstract method __complex__."""

    __slots__ = ()
    @abstractmethod
    def __complex__(self) -> complex: ...

@runtime_checkable
class SupportsBytes(Protocol, metaclass=ABCMeta):
    """An ABC with one abstract method __bytes__."""

    __slots__ = ()
    @abstractmethod
    def __bytes__(self) -> bytes: ...

@runtime_checkable
class SupportsIndex(Protocol, metaclass=ABCMeta):
    """An ABC with one abstract method __index__."""

    __slots__ = ()
    @abstractmethod
    def __index__(self) -> int: ...

@runtime_checkable
class SupportsAbs(Protocol[_T_co]):
    """An ABC with one abstract method __abs__ that is covariant in its return type."""

    __slots__ = ()
    @abstractmethod
    def __abs__(self) -> _T_co: ...

@runtime_checkable
class SupportsRound(Protocol[_T_co]):
    """An ABC with one abstract method __round__ that is covariant in its return type."""

    __slots__ = ()
    @overload
    @abstractmethod
    def __round__(self) -> int: ...
    @overload
    @abstractmethod
    def __round__(self, ndigits: int, /) -> _T_co: ...

@runtime_checkable
class Sized(Protocol, metaclass=ABCMeta):
    @abstractmethod
    def __len__(self) -> int: ...

@runtime_checkable
class Hashable(Protocol, metaclass=ABCMeta):
    # TODO: This is special, in that a subclass of a hashable class may not be hashable
    #   (for example, list vs. object). It's not obvious how to represent this. This class
    #   is currently mostly useless for static checking.
    @abstractmethod
    def __hash__(self) -> int: ...

@runtime_checkable
class Iterable(Protocol[_T_co]):
    @abstractmethod
    def __iter__(self) -> Iterator[_T_co]: ...

@runtime_checkable
class Iterator(Iterable[_T_co], Protocol[_T_co]):
    @abstractmethod
    def __next__(self) -> _T_co:
        """Return the next item from the iterator. When exhausted, raise StopIteration"""

    def __iter__(self) -> Iterator[_T_co]: ...

@runtime_checkable
class Reversible(Iterable[_T_co], Protocol[_T_co]):
    @abstractmethod
    def __reversed__(self) -> Iterator[_T_co]: ...

_YieldT_co = TypeVar("_YieldT_co", covariant=True)
_SendT_contra = TypeVar("_SendT_contra", contravariant=True, default=None)
_ReturnT_co = TypeVar("_ReturnT_co", covariant=True, default=None)

@runtime_checkable
class Generator(Iterator[_YieldT_co], Protocol[_YieldT_co, _SendT_contra, _ReturnT_co]):
    def __next__(self) -> _YieldT_co:
        """Return the next item from the generator.
        When exhausted, raise StopIteration.
        """

    @abstractmethod
    def send(self, value: _SendT_contra, /) -> _YieldT_co:
        """Send a value into the generator.
        Return next yielded value or raise StopIteration.
        """

    @overload
    @abstractmethod
    def throw(
        self, typ: type[BaseException], val: BaseException | object = None, tb: TracebackType | None = None, /
    ) -> _YieldT_co:
        """Raise an exception in the generator.
        Return next yielded value or raise StopIteration.
        """

    @overload
    @abstractmethod
    def throw(self, typ: BaseException, val: None = None, tb: TracebackType | None = None, /) -> _YieldT_co: ...
    if sys.version_info >= (3, 13):
        def close(self) -> _ReturnT_co | None:
            """Raise GeneratorExit inside generator."""
    else:
        def close(self) -> None:
            """Raise GeneratorExit inside generator."""

    def __iter__(self) -> Generator[_YieldT_co, _SendT_contra, _ReturnT_co]: ...

# NOTE: Prior to Python 3.13 these aliases are lacking the second _ExitT_co parameter
if sys.version_info >= (3, 13):
    from contextlib import AbstractAsyncContextManager as AsyncContextManager, AbstractContextManager as ContextManager
else:
    from contextlib import AbstractAsyncContextManager, AbstractContextManager

    @runtime_checkable
    class ContextManager(AbstractContextManager[_T_co, bool | None], Protocol[_T_co]):
        """A generic version of contextlib.AbstractContextManager."""

    @runtime_checkable
    class AsyncContextManager(AbstractAsyncContextManager[_T_co, bool | None], Protocol[_T_co]):
        """A generic version of contextlib.AbstractAsyncContextManager."""

@runtime_checkable
class Awaitable(Protocol[_T_co]):
    @abstractmethod
    def __await__(self) -> Generator[Any, Any, _T_co]: ...

# Non-default variations to accommodate coroutines, and `AwaitableGenerator` having a 4th type parameter.
_SendT_nd_contra = TypeVar("_SendT_nd_contra", contravariant=True)
_ReturnT_nd_co = TypeVar("_ReturnT_nd_co", covariant=True)

class Coroutine(Awaitable[_ReturnT_nd_co], Generic[_YieldT_co, _SendT_nd_contra, _ReturnT_nd_co]):
    __name__: str
    __qualname__: str

    @abstractmethod
    def send(self, value: _SendT_nd_contra, /) -> _YieldT_co:
        """Send a value into the coroutine.
        Return next yielded value or raise StopIteration.
        """

    @overload
    @abstractmethod
    def throw(
        self, typ: type[BaseException], val: BaseException | object = None, tb: TracebackType | None = None, /
    ) -> _YieldT_co:
        """Raise an exception in the coroutine.
        Return next yielded value or raise StopIteration.
        """

    @overload
    @abstractmethod
    def throw(self, typ: BaseException, val: None = None, tb: TracebackType | None = None, /) -> _YieldT_co: ...
    @abstractmethod
    def close(self) -> None:
        """Raise GeneratorExit inside coroutine."""

# NOTE: This type does not exist in typing.py or PEP 484 but mypy needs it to exist.
# The parameters correspond to Generator, but the 4th is the original type.
# Obsolete, use _typeshed._type_checker_internals.AwaitableGenerator instead.
@type_check_only
class AwaitableGenerator(
    Awaitable[_ReturnT_nd_co],
    Generator[_YieldT_co, _SendT_nd_contra, _ReturnT_nd_co],
    Generic[_YieldT_co, _SendT_nd_contra, _ReturnT_nd_co, _S],
    metaclass=ABCMeta,
): ...

@runtime_checkable
class AsyncIterable(Protocol[_T_co]):
    @abstractmethod
    def __aiter__(self) -> AsyncIterator[_T_co]: ...

@runtime_checkable
class AsyncIterator(AsyncIterable[_T_co], Protocol[_T_co]):
    @abstractmethod
    def __anext__(self) -> Awaitable[_T_co]:
        """Return the next item or raise StopAsyncIteration when exhausted."""

    def __aiter__(self) -> AsyncIterator[_T_co]: ...

@runtime_checkable
class AsyncGenerator(AsyncIterator[_YieldT_co], Protocol[_YieldT_co, _SendT_contra]):
    def __anext__(self) -> Coroutine[Any, Any, _YieldT_co]:
        """Return the next item from the asynchronous generator.
        When exhausted, raise StopAsyncIteration.
        """

    @abstractmethod
    def asend(self, value: _SendT_contra, /) -> Coroutine[Any, Any, _YieldT_co]:
        """Send a value into the asynchronous generator.
        Return next yielded value or raise StopAsyncIteration.
        """

    @overload
    @abstractmethod
    def athrow(
        self, typ: type[BaseException], val: BaseException | object = None, tb: TracebackType | None = None, /
    ) -> Coroutine[Any, Any, _YieldT_co]:
        """Raise an exception in the asynchronous generator.
        Return next yielded value or raise StopAsyncIteration.
        """

    @overload
    @abstractmethod
    def athrow(
        self, typ: BaseException, val: None = None, tb: TracebackType | None = None, /
    ) -> Coroutine[Any, Any, _YieldT_co]: ...
    def aclose(self) -> Coroutine[Any, Any, None]:
        """Raise GeneratorExit inside coroutine."""

@runtime_checkable
class Container(Protocol[_T_co]):
    # This is generic more on vibes than anything else
    @abstractmethod
    def __contains__(self, x: object, /) -> bool: ...

@runtime_checkable
class Collection(Iterable[_T_co], Container[_T_co], Protocol[_T_co]):
    # Implement Sized (but don't have it as a base class).
    @abstractmethod
    def __len__(self) -> int: ...

class Sequence(Reversible[_T_co], Collection[_T_co]):
    """All the operations on a read-only sequence.

    Concrete subclasses must override __new__ or __init__,
    __getitem__, and __len__.
    """

    @overload
    @abstractmethod
    def __getitem__(self, index: int) -> _T_co: ...
    @overload
    @abstractmethod
    def __getitem__(self, index: slice) -> Sequence[_T_co]: ...
    # Mixin methods
    def index(self, value: Any, start: int = 0, stop: int = ...) -> int:
        """S.index(value, [start, [stop]]) -> integer -- return first index of value.
        Raises ValueError if the value is not present.

        Supporting start and stop arguments is optional, but
        recommended.
        """

    def count(self, value: Any) -> int:
        """S.count(value) -> integer -- return number of occurrences of value"""

    def __contains__(self, value: object) -> bool: ...
    def __iter__(self) -> Iterator[_T_co]: ...
    def __reversed__(self) -> Iterator[_T_co]: ...

class MutableSequence(Sequence[_T]):
    """All the operations on a read-write sequence.

    Concrete subclasses must provide __new__ or __init__,
    __getitem__, __setitem__, __delitem__, __len__, and insert().
    """

    @abstractmethod
    def insert(self, index: int, value: _T) -> None:
        """S.insert(index, value) -- insert value before index"""

    @overload
    @abstractmethod
    def __getitem__(self, index: int) -> _T: ...
    @overload
    @abstractmethod
    def __getitem__(self, index: slice) -> MutableSequence[_T]: ...
    @overload
    @abstractmethod
    def __setitem__(self, index: int, value: _T) -> None: ...
    @overload
    @abstractmethod
    def __setitem__(self, index: slice, value: Iterable[_T]) -> None: ...
    @overload
    @abstractmethod
    def __delitem__(self, index: int) -> None: ...
    @overload
    @abstractmethod
    def __delitem__(self, index: slice) -> None: ...
    # Mixin methods
    def append(self, value: _T) -> None:
        """S.append(value) -- append value to the end of the sequence"""

    def clear(self) -> None:
        """S.clear() -> None -- remove all items from S"""

    def extend(self, values: Iterable[_T]) -> None:
        """S.extend(iterable) -- extend sequence by appending elements from the iterable"""

    def reverse(self) -> None:
        """S.reverse() -- reverse *IN PLACE*"""

    def pop(self, index: int = -1) -> _T:
        """S.pop([index]) -> item -- remove and return item at index (default last).
        Raise IndexError if list is empty or index is out of range.
        """

    def remove(self, value: _T) -> None:
        """S.remove(value) -- remove first occurrence of value.
        Raise ValueError if the value is not present.
        """

    def __iadd__(self, values: Iterable[_T]) -> typing_extensions.Self: ...

class AbstractSet(Collection[_T_co]):
    """A set is a finite, iterable container.

    This class provides concrete generic implementations of all
    methods except for __contains__, __iter__ and __len__.

    To override the comparisons (presumably for speed, as the
    semantics are fixed), redefine __le__ and __ge__,
    then the other operations will automatically follow suit.
    """

    @abstractmethod
    def __contains__(self, x: object) -> bool: ...
    def _hash(self) -> int:
        """Compute the hash value of a set.

        Note that we don't define __hash__: not all sets are hashable.
        But if you define a hashable set type, its __hash__ should
        call this function.

        This must be compatible __eq__.

        All sets ought to compare equal if they contain the same
        elements, regardless of how they are implemented, and
        regardless of the order of the elements; so there's not much
        freedom for __eq__ or __hash__.  We match the algorithm used
        by the built-in frozenset type.
        """
    # Mixin methods
    def __le__(self, other: AbstractSet[Any]) -> bool: ...
    def __lt__(self, other: AbstractSet[Any]) -> bool: ...
    def __gt__(self, other: AbstractSet[Any]) -> bool: ...
    def __ge__(self, other: AbstractSet[Any]) -> bool: ...
    def __and__(self, other: AbstractSet[Any]) -> AbstractSet[_T_co]: ...
    def __or__(self, other: AbstractSet[_T]) -> AbstractSet[_T_co | _T]: ...
    def __sub__(self, other: AbstractSet[Any]) -> AbstractSet[_T_co]: ...
    def __xor__(self, other: AbstractSet[_T]) -> AbstractSet[_T_co | _T]: ...
    def __eq__(self, other: object) -> bool: ...
    def isdisjoint(self, other: Iterable[Any]) -> bool:
        """Return True if two sets have a null intersection."""

class MutableSet(AbstractSet[_T]):
    """A mutable set is a finite, iterable container.

    This class provides concrete generic implementations of all
    methods except for __contains__, __iter__, __len__,
    add(), and discard().

    To override the comparisons (presumably for speed, as the
    semantics are fixed), all you have to do is redefine __le__ and
    then the other operations will automatically follow suit.
    """

    @abstractmethod
    def add(self, value: _T) -> None:
        """Add an element."""

    @abstractmethod
    def discard(self, value: _T) -> None:
        """Remove an element.  Do not raise an exception if absent."""
    # Mixin methods
    def clear(self) -> None:
        """This is slow (creates N new iterators!) but effective."""

    def pop(self) -> _T:
        """Return the popped value.  Raise KeyError if empty."""

    def remove(self, value: _T) -> None:
        """Remove an element. If not a member, raise a KeyError."""

    def __ior__(self, it: AbstractSet[_T]) -> typing_extensions.Self: ...  # type: ignore[override,misc]
    def __iand__(self, it: AbstractSet[Any]) -> typing_extensions.Self: ...
    def __ixor__(self, it: AbstractSet[_T]) -> typing_extensions.Self: ...  # type: ignore[override,misc]
    def __isub__(self, it: AbstractSet[Any]) -> typing_extensions.Self: ...

class MappingView(Sized):
    __slots__ = ("_mapping",)
    def __init__(self, mapping: Sized) -> None: ...  # undocumented
    def __len__(self) -> int: ...

class ItemsView(MappingView, AbstractSet[tuple[_KT_co, _VT_co]], Generic[_KT_co, _VT_co]):
    def __init__(self, mapping: SupportsGetItemViewable[_KT_co, _VT_co]) -> None: ...  # undocumented
    def __and__(self, other: Iterable[Any]) -> set[tuple[_KT_co, _VT_co]]: ...
    def __rand__(self, other: Iterable[_T]) -> set[_T]: ...
    def __contains__(self, item: tuple[object, object]) -> bool: ...  # type: ignore[override]
    def __iter__(self) -> Iterator[tuple[_KT_co, _VT_co]]: ...
    def __or__(self, other: Iterable[_T]) -> set[tuple[_KT_co, _VT_co] | _T]: ...
    def __ror__(self, other: Iterable[_T]) -> set[tuple[_KT_co, _VT_co] | _T]: ...
    def __sub__(self, other: Iterable[Any]) -> set[tuple[_KT_co, _VT_co]]: ...
    def __rsub__(self, other: Iterable[_T]) -> set[_T]: ...
    def __xor__(self, other: Iterable[_T]) -> set[tuple[_KT_co, _VT_co] | _T]: ...
    def __rxor__(self, other: Iterable[_T]) -> set[tuple[_KT_co, _VT_co] | _T]: ...

class KeysView(MappingView, AbstractSet[_KT_co]):
    def __init__(self, mapping: Viewable[_KT_co]) -> None: ...  # undocumented
    def __and__(self, other: Iterable[Any]) -> set[_KT_co]: ...
    def __rand__(self, other: Iterable[_T]) -> set[_T]: ...
    def __contains__(self, key: object) -> bool: ...
    def __iter__(self) -> Iterator[_KT_co]: ...
    def __or__(self, other: Iterable[_T]) -> set[_KT_co | _T]: ...
    def __ror__(self, other: Iterable[_T]) -> set[_KT_co | _T]: ...
    def __sub__(self, other: Iterable[Any]) -> set[_KT_co]: ...
    def __rsub__(self, other: Iterable[_T]) -> set[_T]: ...
    def __xor__(self, other: Iterable[_T]) -> set[_KT_co | _T]: ...
    def __rxor__(self, other: Iterable[_T]) -> set[_KT_co | _T]: ...

class ValuesView(MappingView, Collection[_VT_co]):
    def __init__(self, mapping: SupportsGetItemViewable[Any, _VT_co]) -> None: ...  # undocumented
    def __contains__(self, value: object) -> bool: ...
    def __iter__(self) -> Iterator[_VT_co]: ...

# note for Mapping.get and MutableMapping.pop and MutableMapping.setdefault
# In _collections_abc.py the parameters are positional-or-keyword,
# but dict and types.MappingProxyType (the vast majority of Mapping types)
# don't allow keyword arguments.

class Mapping(Collection[_KT], Generic[_KT, _VT_co]):
    """A Mapping is a generic container for associating key/value
    pairs.

    This class provides concrete generic implementations of all
    methods except for __getitem__, __iter__, and __len__.
    """

    # TODO: We wish the key type could also be covariant, but that doesn't work,
    # see discussion in https://github.com/python/typing/pull/273.
    @abstractmethod
    def __getitem__(self, key: _KT, /) -> _VT_co: ...
    # Mixin methods
    @overload
    def get(self, key: _KT, /) -> _VT_co | None:
        """D.get(k[,d]) -> D[k] if k in D, else d.  d defaults to None."""

    @overload
    def get(self, key: _KT, default: _VT_co, /) -> _VT_co: ...  # type: ignore[misc] # pyright: ignore[reportGeneralTypeIssues] # Covariant type as parameter
    @overload
    def get(self, key: _KT, default: _T, /) -> _VT_co | _T: ...
    def items(self) -> ItemsView[_KT, _VT_co]:
        """D.items() -> a set-like object providing a view on D's items"""

    def keys(self) -> KeysView[_KT]:
        """D.keys() -> a set-like object providing a view on D's keys"""

    def values(self) -> ValuesView[_VT_co]:
        """D.values() -> an object providing a view on D's values"""

    def __contains__(self, key: object, /) -> bool: ...
    def __eq__(self, other: object, /) -> bool: ...

class MutableMapping(Mapping[_KT, _VT]):
    """A MutableMapping is a generic container for associating
    key/value pairs.

    This class provides concrete generic implementations of all
    methods except for __getitem__, __setitem__, __delitem__,
    __iter__, and __len__.
    """

    @abstractmethod
    def __setitem__(self, key: _KT, value: _VT, /) -> None: ...
    @abstractmethod
    def __delitem__(self, key: _KT, /) -> None: ...
    def clear(self) -> None:
        """D.clear() -> None.  Remove all items from D."""

    @overload
    def pop(self, key: _KT, /) -> _VT:
        """D.pop(k[,d]) -> v, remove specified key and return the corresponding value.
        If key is not found, d is returned if given, otherwise KeyError is raised.
        """

    @overload
    def pop(self, key: _KT, default: _VT, /) -> _VT: ...
    @overload
    def pop(self, key: _KT, default: _T, /) -> _VT | _T: ...
    def popitem(self) -> tuple[_KT, _VT]:
        """D.popitem() -> (k, v), remove and return some (key, value) pair
        as a 2-tuple; but raise KeyError if D is empty.
        """
    # This overload should be allowed only if the value type is compatible with None.
    #
    # Keep the following methods in line with MutableMapping.setdefault, modulo positional-only differences:
    # -- collections.OrderedDict.setdefault
    # -- collections.ChainMap.setdefault
    # -- weakref.WeakKeyDictionary.setdefault
    @overload
    def setdefault(self: MutableMapping[_KT, _T | None], key: _KT, default: None = None, /) -> _T | None:
        """D.setdefault(k[,d]) -> D.get(k,d), also set D[k]=d if k not in D"""

    @overload
    def setdefault(self, key: _KT, default: _VT, /) -> _VT: ...
    # 'update' used to take a Union, but using overloading is better.
    # The second overloaded type here is a bit too general, because
    # Mapping[tuple[_KT, _VT], W] is a subclass of Iterable[tuple[_KT, _VT]],
    # but will always have the behavior of the first overloaded type
    # at runtime, leading to keys of a mix of types _KT and tuple[_KT, _VT].
    # We don't currently have any way of forcing all Mappings to use
    # the first overload, but by using overloading rather than a Union,
    # mypy will commit to using the first overload when the argument is
    # known to be a Mapping with unknown type parameters, which is closer
    # to the behavior we want. See mypy issue  #1430.
    #
    # Various mapping classes have __ior__ methods that should be kept roughly in line with .update():
    # -- dict.__ior__
    # -- os._Environ.__ior__
    # -- collections.UserDict.__ior__
    # -- collections.ChainMap.__ior__
    # -- peewee.attrdict.__add__
    # -- peewee.attrdict.__iadd__
    # -- weakref.WeakValueDictionary.__ior__
    # -- weakref.WeakKeyDictionary.__ior__
    @overload
    def update(self, m: SupportsKeysAndGetItem[_KT, _VT], /) -> None:
        """D.update([E, ]**F) -> None.  Update D from mapping/iterable E and F.
        If E present and has a .keys() method, does:     for k in E.keys(): D[k] = E[k]
        If E present and lacks .keys() method, does:     for (k, v) in E: D[k] = v
        In either case, this is followed by: for k, v in F.items(): D[k] = v
        """

    @overload
    def update(self: SupportsGetItem[str, _VT], m: SupportsKeysAndGetItem[str, _VT], /, **kwargs: _VT) -> None: ...
    @overload
    def update(self, m: Iterable[tuple[_KT, _VT]], /) -> None: ...
    @overload
    def update(self: SupportsGetItem[str, _VT], m: Iterable[tuple[str, _VT]], /, **kwargs: _VT) -> None: ...
    @overload
    def update(self: SupportsGetItem[str, _VT], **kwargs: _VT) -> None: ...

Text = str

TYPE_CHECKING: Final[bool]

# In stubs, the arguments of the IO class are marked as positional-only.
# This differs from runtime, but better reflects the fact that in reality
# classes deriving from IO use different names for the arguments.
class IO(Generic[AnyStr]):
    """Generic base class for TextIO and BinaryIO.

    This is an abstract, generic version of the return of open().

    NOTE: This does not distinguish between the different possible
    classes (text vs. binary, read vs. write vs. read/write,
    append-only, unbuffered).  The TextIO and BinaryIO subclasses
    below capture the distinctions between text vs. binary, which is
    pervasive in the interface; however we currently do not offer a
    way to track the other distinctions in the type system.
    """

    # At runtime these are all abstract properties,
    # but making them abstract in the stub is hugely disruptive, for not much gain.
    # See #8726
    __slots__ = ()
    @property
    def mode(self) -> str: ...
    # Usually str, but may be bytes if a bytes path was passed to open(). See #10737.
    # If PEP 696 becomes available, we may want to use a defaulted TypeVar here.
    @property
    def name(self) -> str | Any: ...
    @abstractmethod
    def close(self) -> None: ...
    @property
    def closed(self) -> bool: ...
    @abstractmethod
    def fileno(self) -> int: ...
    @abstractmethod
    def flush(self) -> None: ...
    @abstractmethod
    def isatty(self) -> bool: ...
    @abstractmethod
    def read(self, n: int = -1, /) -> AnyStr: ...
    @abstractmethod
    def readable(self) -> bool: ...
    @abstractmethod
    def readline(self, limit: int = -1, /) -> AnyStr: ...
    @abstractmethod
    def readlines(self, hint: int = -1, /) -> list[AnyStr]: ...
    @abstractmethod
    def seek(self, offset: int, whence: int = 0, /) -> int: ...
    @abstractmethod
    def seekable(self) -> bool: ...
    @abstractmethod
    def tell(self) -> int: ...
    @abstractmethod
    def truncate(self, size: int | None = None, /) -> int: ...
    @abstractmethod
    def writable(self) -> bool: ...
    @abstractmethod
    @overload
    def write(self: IO[bytes], s: ReadableBuffer, /) -> int: ...
    @abstractmethod
    @overload
    def write(self, s: AnyStr, /) -> int: ...
    @abstractmethod
    @overload
    def writelines(self: IO[bytes], lines: Iterable[ReadableBuffer], /) -> None: ...
    @abstractmethod
    @overload
    def writelines(self, lines: Iterable[AnyStr], /) -> None: ...
    @abstractmethod
    def __next__(self) -> AnyStr: ...
    @abstractmethod
    def __iter__(self) -> Iterator[AnyStr]: ...
    @abstractmethod
    def __enter__(self) -> IO[AnyStr]: ...
    @abstractmethod
    def __exit__(
        self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None, /
    ) -> None: ...

class BinaryIO(IO[bytes]):
    """Typed version of the return of open() in binary mode."""

    __slots__ = ()
    @abstractmethod
    def __enter__(self) -> BinaryIO: ...

class TextIO(IO[str]):
    """Typed version of the return of open() in text mode."""

    # See comment regarding the @properties in the `IO` class
    __slots__ = ()
    @property
    def buffer(self) -> BinaryIO: ...
    @property
    def encoding(self) -> str: ...
    @property
    def errors(self) -> str | None: ...
    @property
    def line_buffering(self) -> int: ...  # int on PyPy, bool on CPython
    @property
    def newlines(self) -> Any: ...  # None, str or tuple
    @abstractmethod
    def __enter__(self) -> TextIO: ...

ByteString: typing_extensions.TypeAlias = bytes | bytearray | memoryview

# Functions

_get_type_hints_obj_allowed_types: typing_extensions.TypeAlias = (  # noqa: Y042
    object
    | Callable[..., Any]
    | FunctionType
    | BuiltinFunctionType
    | MethodType
    | ModuleType
    | WrapperDescriptorType
    | MethodWrapperType
    | MethodDescriptorType
)

if sys.version_info >= (3, 14):
    def get_type_hints(
        obj: _get_type_hints_obj_allowed_types,
        globalns: dict[str, Any] | None = None,
        localns: Mapping[str, Any] | None = None,
        include_extras: bool = False,
        *,
        format: Format | None = None,
    ) -> dict[str, Any]:  # AnnotationForm
        """Return type hints for an object.

        This is often the same as obj.__annotations__, but it handles
        forward references encoded as string literals and recursively replaces all
        'Annotated[T, ...]' with 'T' (unless 'include_extras=True').

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
          to have globals, an empty dictionary is used.  For classes, the search
          order is globals first then locals.

        - If one dict argument is passed, it is used for both globals and
          locals.

        - If two dict arguments are passed, they specify globals and
          locals, respectively.
        """

else:
    def get_type_hints(
        obj: _get_type_hints_obj_allowed_types,
        globalns: dict[str, Any] | None = None,
        localns: Mapping[str, Any] | None = None,
        include_extras: bool = False,
    ) -> dict[str, Any]:  # AnnotationForm
        """Return type hints for an object.

        This is often the same as obj.__annotations__, but it handles
        forward references encoded as string literals and recursively replaces all
        'Annotated[T, ...]' with 'T' (unless 'include_extras=True').

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
          to have globals, an empty dictionary is used.  For classes, the search
          order is globals first then locals.

        - If one dict argument is passed, it is used for both globals and
          locals.

        - If two dict arguments are passed, they specify globals and
          locals, respectively.
        """

def get_args(tp: Any) -> tuple[Any, ...]:  # AnnotationForm
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
    def get_origin(tp: ParamSpecArgs | ParamSpecKwargs) -> ParamSpec:
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
    def get_origin(tp: UnionType) -> type[UnionType]: ...

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
def get_origin(tp: Any) -> Any | None: ...  # AnnotationForm
@overload
def cast(typ: type[_T], val: Any) -> _T:
    """Cast a value to a type.

    This returns the value unchanged.  To the type checker this
    signals that the return value has the designated type, but at
    runtime we intentionally don't check anything (we want this
    to be as fast as possible).
    """

@overload
def cast(typ: str, val: Any) -> Any: ...
@overload
def cast(typ: object, val: Any) -> Any: ...

if sys.version_info >= (3, 11):
    def reveal_type(obj: _T, /) -> _T:
        """Ask a static type checker to reveal the inferred type of an expression.

        When a static type checker encounters a call to ``reveal_type()``,
        it will emit the inferred type of the argument::

            x: int = 1
            reveal_type(x)

        Running a static type checker (e.g., mypy) on this example
        will produce output similar to 'Revealed type is "builtins.int"'.

        At runtime, the function prints the runtime type of the
        argument and returns the argument unchanged.
        """

    def assert_never(arg: Never, /) -> Never:
        """Statically assert that a line of code is unreachable.

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

    def assert_type(val: _T, typ: Any, /) -> _T:  # AnnotationForm
        """Ask a static type checker to confirm that the value is of the given type.

        At runtime this does nothing: it returns the first argument unchanged with no
        checks or side effects, no matter the actual type of the argument.

        When a static type checker encounters a call to assert_type(), it
        emits an error if the value is not of the specified type::

            def greet(name: str) -> None:
                assert_type(name, str)  # OK
                assert_type(name, int)  # type checker error
        """

    def clear_overloads() -> None:
        """Clear all overloads in the registry."""

    def get_overloads(func: Callable[..., object]) -> Sequence[Callable[..., object]]:
        """Return all defined overloads for *func* as a sequence."""

    def dataclass_transform(
        *,
        eq_default: bool = True,
        order_default: bool = False,
        kw_only_default: bool = False,
        frozen_default: bool = False,  # on 3.11, runtime accepts it as part of kwargs
        field_specifiers: tuple[type[Any] | Callable[..., Any], ...] = (),
        **kwargs: Any,
    ) -> IdentityFunction:
        """Decorator to mark an object as providing dataclass-like behaviour.

        The decorator can be applied to a function, class, or metaclass.

        Example usage with a decorator function::

            @dataclass_transform()
            def create_model[T](cls: type[T]) -> type[T]:
                ...
                return cls

            @create_model
            class CustomerModel:
                id: int
                name: str

        On a base class::

            @dataclass_transform()
            class ModelBase: ...

            class CustomerModel(ModelBase):
                id: int
                name: str

        On a metaclass::

            @dataclass_transform()
            class ModelMeta(type): ...

            class ModelBase(metaclass=ModelMeta): ...

            class CustomerModel(ModelBase):
                id: int
                name: str

        The ``CustomerModel`` classes defined above will
        be treated by type checkers similarly to classes created with
        ``@dataclasses.dataclass``.
        For example, type checkers will assume these classes have
        ``__init__`` methods that accept ``id`` and ``name``.

        The arguments to this decorator can be used to customize this behavior:
        - ``eq_default`` indicates whether the ``eq`` parameter is assumed to be
            ``True`` or ``False`` if it is omitted by the caller.
        - ``order_default`` indicates whether the ``order`` parameter is
            assumed to be True or False if it is omitted by the caller.
        - ``kw_only_default`` indicates whether the ``kw_only`` parameter is
            assumed to be True or False if it is omitted by the caller.
        - ``frozen_default`` indicates whether the ``frozen`` parameter is
            assumed to be True or False if it is omitted by the caller.
        - ``field_specifiers`` specifies a static list of supported classes
            or functions that describe fields, similar to ``dataclasses.field()``.
        - Arbitrary other keyword arguments are accepted in order to allow for
            possible future extensions.

        At runtime, this decorator records its arguments in the
        ``__dataclass_transform__`` attribute on the decorated object.
        It has no other runtime effect.

        See PEP 681 for more details.
        """

# Type constructors

# Obsolete, will be changed to a function. Use _typeshed._type_checker_internals.NamedTupleFallback instead.
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
    # __orig_bases__ sometimes exists on <3.12, but not consistently
    # So we only add it to the stub on 3.12+.
    if sys.version_info >= (3, 12):
        __orig_bases__: ClassVar[tuple[Any, ...]]

    @overload
    def __init__(self, typename: str, fields: Iterable[tuple[str, Any]], /) -> None: ...
    @overload
    @deprecated("Creating a typing.NamedTuple using keyword arguments is deprecated and support will be removed in Python 3.15")
    def __init__(self, typename: str, fields: None = None, /, **kwargs: Any) -> None: ...
    @classmethod
    def _make(cls, iterable: Iterable[Any]) -> typing_extensions.Self: ...
    def _asdict(self) -> dict[str, Any]: ...
    def _replace(self, **kwargs: Any) -> typing_extensions.Self: ...
    if sys.version_info >= (3, 13):
        def __replace__(self, **kwargs: Any) -> typing_extensions.Self: ...

# Internal mypy fallback type for all typed dicts (does not exist at runtime)
# N.B. Keep this mostly in sync with typing_extensions._TypedDict/mypy_extensions._TypedDict
# Obsolete, use _typeshed._type_checker_internals.TypedDictFallback instead.
@type_check_only
class _TypedDict(Mapping[str, object], metaclass=ABCMeta):
    __total__: ClassVar[bool]
    __required_keys__: ClassVar[frozenset[str]]
    __optional_keys__: ClassVar[frozenset[str]]
    # __orig_bases__ sometimes exists on <3.12, but not consistently,
    # so we only add it to the stub on 3.12+
    if sys.version_info >= (3, 12):
        __orig_bases__: ClassVar[tuple[Any, ...]]
    if sys.version_info >= (3, 13):
        __readonly_keys__: ClassVar[frozenset[str]]
        __mutable_keys__: ClassVar[frozenset[str]]

    def copy(self) -> typing_extensions.Self: ...
    # Using Never so that only calls using mypy plugin hook that specialize the signature
    # can go through.
    def setdefault(self, k: _Never, default: object) -> object: ...
    # Mypy plugin hook for 'pop' expects that 'default' has a type variable type.
    def pop(self, k: _Never, default: _T = ...) -> object: ...  # pyright: ignore[reportInvalidTypeVarUse]
    def update(self, m: typing_extensions.Self, /) -> None: ...
    def __delitem__(self, k: _Never) -> None: ...
    def items(self) -> dict_items[str, object]: ...
    def keys(self) -> dict_keys[str, object]: ...
    def values(self) -> dict_values[str, object]: ...
    @overload
    def __or__(self, value: typing_extensions.Self, /) -> typing_extensions.Self:
        """Return self|value."""

    @overload
    def __or__(self, value: dict[str, Any], /) -> dict[str, object]: ...
    @overload
    def __ror__(self, value: typing_extensions.Self, /) -> typing_extensions.Self:
        """Return value|self."""

    @overload
    def __ror__(self, value: dict[str, Any], /) -> dict[str, object]: ...
    # supposedly incompatible definitions of __or__ and __ior__
    def __ior__(self, value: typing_extensions.Self, /) -> typing_extensions.Self: ...  # type: ignore[misc]

if sys.version_info >= (3, 14):
    from annotationlib import ForwardRef as ForwardRef

    def evaluate_forward_ref(
        forward_ref: ForwardRef,
        *,
        owner: object = None,
        globals: dict[str, Any] | None = None,
        locals: Mapping[str, Any] | None = None,
        type_params: tuple[TypeVar, ParamSpec, TypeVarTuple] | None = None,
        format: Format | None = None,
    ) -> Any:  # AnnotationForm
        """Evaluate a forward reference as a type hint.

        This is similar to calling the ForwardRef.evaluate() method,
        but unlike that method, evaluate_forward_ref() also
        recursively evaluates forward references nested within the type hint.

        *forward_ref* must be an instance of ForwardRef. *owner*, if given,
        should be the object that holds the annotations that the forward reference
        derived from, such as a module, class object, or function. It is used to
        infer the namespaces to use for looking up names. *globals* and *locals*
        can also be explicitly given to provide the global and local namespaces.
        *type_params* is a tuple of type parameters that are in scope when
        evaluating the forward reference. This parameter should be provided (though
        it may be an empty tuple) if *owner* is not given and the forward reference
        does not already have an owner set. *format* specifies the format of the
        annotation and is a member of the annotationlib.Format enum, defaulting to
        VALUE.

        """

else:
    @final
    class ForwardRef(_Final):
        """Internal wrapper to hold a forward reference."""

        __slots__ = (
            "__forward_arg__",
            "__forward_code__",
            "__forward_evaluated__",
            "__forward_value__",
            "__forward_is_argument__",
            "__forward_is_class__",
            "__forward_module__",
        )
        __forward_arg__: str
        __forward_code__: CodeType
        __forward_evaluated__: bool
        __forward_value__: Any | None  # AnnotationForm
        __forward_is_argument__: bool
        __forward_is_class__: bool
        __forward_module__: Any | None

        def __init__(self, arg: str, is_argument: bool = True, module: Any | None = None, *, is_class: bool = False) -> None: ...

        if sys.version_info >= (3, 13):
            @overload
            @deprecated(
                "Failing to pass a value to the 'type_params' parameter of ForwardRef._evaluate() is deprecated, "
                "as it leads to incorrect behaviour when evaluating a stringified annotation "
                "that references a PEP 695 type parameter. It will be disallowed in Python 3.15."
            )
            def _evaluate(
                self, globalns: dict[str, Any] | None, localns: Mapping[str, Any] | None, *, recursive_guard: frozenset[str]
            ) -> Any | None: ...  # AnnotationForm
            @overload
            def _evaluate(
                self,
                globalns: dict[str, Any] | None,
                localns: Mapping[str, Any] | None,
                type_params: tuple[TypeVar | ParamSpec | TypeVarTuple, ...],
                *,
                recursive_guard: frozenset[str],
            ) -> Any | None: ...  # AnnotationForm
        elif sys.version_info >= (3, 12):
            def _evaluate(
                self,
                globalns: dict[str, Any] | None,
                localns: Mapping[str, Any] | None,
                type_params: tuple[TypeVar | ParamSpec | TypeVarTuple, ...] | None = None,
                *,
                recursive_guard: frozenset[str],
            ) -> Any | None: ...  # AnnotationForm
        else:
            def _evaluate(
                self, globalns: dict[str, Any] | None, localns: Mapping[str, Any] | None, recursive_guard: frozenset[str]
            ) -> Any | None: ...  # AnnotationForm

        def __eq__(self, other: object) -> bool: ...
        def __hash__(self) -> int: ...
        if sys.version_info >= (3, 11):
            def __or__(self, other: Any) -> _SpecialForm: ...
            def __ror__(self, other: Any) -> _SpecialForm: ...

if sys.version_info >= (3, 10):
    def is_typeddict(tp: object) -> bool:
        """Check if an annotation is a TypedDict class.

        For example::

            >>> from typing import TypedDict
            >>> class Film(TypedDict):
            ...     title: str
            ...     year: int
            ...
            >>> is_typeddict(Film)
            True
            >>> is_typeddict(dict)
            False
        """

def _type_repr(obj: object) -> str:
    """Return the repr() of an object, special-casing types (internal helper).

    If obj is a type, we return a shorter version than the default
    type.__repr__, based on the module and qualified name, which is
    typically enough to uniquely identify a type.  For everything
    else, we fall back on repr(obj).
    """

if sys.version_info >= (3, 12):
    _TypeParameter: typing_extensions.TypeAlias = (
        TypeVar
        | typing_extensions.TypeVar
        | ParamSpec
        | typing_extensions.ParamSpec
        | TypeVarTuple
        | typing_extensions.TypeVarTuple
    )

    def override(method: _F, /) -> _F:
        """Indicate that a method is intended to override a method in a base class.

        Usage::

            class Base:
                def method(self) -> None:
                    pass

            class Child(Base):
                @override
                def method(self) -> None:
                    super().method()

        When this decorator is applied to a method, the type checker will
        validate that it overrides a method or attribute with the same name on a
        base class.  This helps prevent bugs that may occur when a base class is
        changed without an equivalent change to a child class.

        There is no runtime checking of this property. The decorator attempts to
        set the ``__override__`` attribute to ``True`` on the decorated object to
        allow runtime introspection.

        See PEP 698 for details.
        """

    @final
    class TypeAliasType:
        """Type alias.

        Type aliases are created through the type statement::

            type Alias = int

        In this example, Alias and int will be treated equivalently by static
        type checkers.

        At runtime, Alias is an instance of TypeAliasType. The __name__
        attribute holds the name of the type alias. The value of the type alias
        is stored in the __value__ attribute. It is evaluated lazily, so the
        value is computed only if the attribute is accessed.

        Type aliases can also be generic::

            type ListOrSet[T] = list[T] | set[T]

        In this case, the type parameters of the alias are stored in the
        __type_params__ attribute.

        See PEP 695 for more information.
        """

        def __new__(cls, name: str, value: Any, *, type_params: tuple[_TypeParameter, ...] = ()) -> Self: ...
        @property
        def __value__(self) -> Any: ...  # AnnotationForm
        @property
        def __type_params__(self) -> tuple[_TypeParameter, ...]: ...
        @property
        def __parameters__(self) -> tuple[Any, ...]: ...  # AnnotationForm
        @property
        def __name__(self) -> str: ...
        # It's writable on types, but not on instances of TypeAliasType.
        @property
        def __module__(self) -> str | None: ...  # type: ignore[override]
        def __getitem__(self, parameters: Any, /) -> GenericAlias:  # AnnotationForm
            """Return self[key]."""

        def __or__(self, right: Any, /) -> _SpecialForm:
            """Return self|value."""

        def __ror__(self, left: Any, /) -> _SpecialForm:
            """Return value|self."""
        if sys.version_info >= (3, 14):
            @property
            def evaluate_value(self) -> EvaluateFunc: ...

if sys.version_info >= (3, 13):
    def is_protocol(tp: type, /) -> bool:
        """Return True if the given type is a Protocol.

        Example::

            >>> from typing import Protocol, is_protocol
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

            >>> from typing import Protocol, get_protocol_members
            >>> class P(Protocol):
            ...     def a(self) -> str: ...
            ...     b: int
            >>> get_protocol_members(P) == frozenset({'a', 'b'})
            True

        Raise a TypeError for arguments that are not Protocols.
        """

    @final
    @type_check_only
    class _NoDefaultType: ...

    NoDefault: _NoDefaultType
    TypeIs: _SpecialForm
    """Special typing construct for marking user-defined type predicate functions.

    ``TypeIs`` can be used to annotate the return type of a user-defined
    type predicate function.  ``TypeIs`` only accepts a single type argument.
    At runtime, functions marked this way should return a boolean and accept
    at least one argument.

    ``TypeIs`` aims to benefit *type narrowing* -- a technique used by static
    type checkers to determine a more precise type of an expression within a
    program's code flow.  Usually type narrowing is done by analyzing
    conditional code flow and applying the narrowing to a block of code.  The
    conditional expression here is sometimes referred to as a "type predicate".

    Sometimes it would be convenient to use a user-defined boolean function
    as a type predicate.  Such a function should use ``TypeIs[...]`` or
    ``TypeGuard[...]`` as its return type to alert static type checkers to
    this intention.  ``TypeIs`` usually has more intuitive behavior than
    ``TypeGuard``, but it cannot be used when the input and output types
    are incompatible (e.g., ``list[object]`` to ``list[int]``) or when the
    function does not return ``True`` for all instances of the narrowed type.

    Using  ``-> TypeIs[NarrowedType]`` tells the static type checker that for
    a given function:

    1. The return value is a boolean.
    2. If the return value is ``True``, the type of its argument
       is the intersection of the argument's original type and
       ``NarrowedType``.
    3. If the return value is ``False``, the type of its argument
       is narrowed to exclude ``NarrowedType``.

    For example::

        from typing import assert_type, final, TypeIs

        class Parent: pass
        class Child(Parent): pass
        @final
        class Unrelated: pass

        def is_parent(val: object) -> TypeIs[Parent]:
            return isinstance(val, Parent)

        def run(arg: Child | Unrelated):
            if is_parent(arg):
                # Type of ``arg`` is narrowed to the intersection
                # of ``Parent`` and ``Child``, which is equivalent to
                # ``Child``.
                assert_type(arg, Child)
            else:
                # Type of ``arg`` is narrowed to exclude ``Parent``,
                # so only ``Unrelated`` is left.
                assert_type(arg, Unrelated)

    The type inside ``TypeIs`` must be consistent with the type of the
    function's argument; if it is not, static type checkers will raise
    an error.  An incorrectly written ``TypeIs`` function can lead to
    unsound behavior in the type system; it is the user's responsibility
    to write such functions in a type-safe manner.

    ``TypeIs`` also works with type variables.  For more information, see
    PEP 742 (Narrowing types with ``TypeIs``).
    """

    ReadOnly: _SpecialForm
    """A special typing construct to mark an item of a TypedDict as read-only.

    For example::

        class Movie(TypedDict):
            title: ReadOnly[str]
            year: int

        def mutate_movie(m: Movie) -> None:
            m["year"] = 1992  # allowed
            m["title"] = "The Matrix"  # typechecker error

    There is no runtime checking for this property.
    """
