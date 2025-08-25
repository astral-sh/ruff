import dis
import enum
import sys
import types
from _typeshed import AnnotationForm, StrPath
from collections import OrderedDict
from collections.abc import AsyncGenerator, Awaitable, Callable, Coroutine, Generator, Mapping, Sequence, Set as AbstractSet
from types import (
    AsyncGeneratorType,
    BuiltinFunctionType,
    BuiltinMethodType,
    ClassMethodDescriptorType,
    CodeType,
    CoroutineType,
    FrameType,
    FunctionType,
    GeneratorType,
    GetSetDescriptorType,
    LambdaType,
    MemberDescriptorType,
    MethodDescriptorType,
    MethodType,
    MethodWrapperType,
    ModuleType,
    TracebackType,
    WrapperDescriptorType,
)
from typing import Any, ClassVar, Final, Literal, NamedTuple, Protocol, TypeVar, overload, type_check_only
from typing_extensions import ParamSpec, Self, TypeAlias, TypeGuard, TypeIs, deprecated, disjoint_base

if sys.version_info >= (3, 14):
    from annotationlib import Format

if sys.version_info >= (3, 11):
    __all__ = [
        "ArgInfo",
        "Arguments",
        "Attribute",
        "BlockFinder",
        "BoundArguments",
        "CORO_CLOSED",
        "CORO_CREATED",
        "CORO_RUNNING",
        "CORO_SUSPENDED",
        "CO_ASYNC_GENERATOR",
        "CO_COROUTINE",
        "CO_GENERATOR",
        "CO_ITERABLE_COROUTINE",
        "CO_NESTED",
        "CO_NEWLOCALS",
        "CO_NOFREE",
        "CO_OPTIMIZED",
        "CO_VARARGS",
        "CO_VARKEYWORDS",
        "ClassFoundException",
        "ClosureVars",
        "EndOfBlock",
        "FrameInfo",
        "FullArgSpec",
        "GEN_CLOSED",
        "GEN_CREATED",
        "GEN_RUNNING",
        "GEN_SUSPENDED",
        "Parameter",
        "Signature",
        "TPFLAGS_IS_ABSTRACT",
        "Traceback",
        "classify_class_attrs",
        "cleandoc",
        "currentframe",
        "findsource",
        "formatannotation",
        "formatannotationrelativeto",
        "formatargvalues",
        "get_annotations",
        "getabsfile",
        "getargs",
        "getargvalues",
        "getattr_static",
        "getblock",
        "getcallargs",
        "getclasstree",
        "getclosurevars",
        "getcomments",
        "getcoroutinelocals",
        "getcoroutinestate",
        "getdoc",
        "getfile",
        "getframeinfo",
        "getfullargspec",
        "getgeneratorlocals",
        "getgeneratorstate",
        "getinnerframes",
        "getlineno",
        "getmembers",
        "getmembers_static",
        "getmodule",
        "getmodulename",
        "getmro",
        "getouterframes",
        "getsource",
        "getsourcefile",
        "getsourcelines",
        "indentsize",
        "isabstract",
        "isasyncgen",
        "isasyncgenfunction",
        "isawaitable",
        "isbuiltin",
        "isclass",
        "iscode",
        "iscoroutine",
        "iscoroutinefunction",
        "isdatadescriptor",
        "isframe",
        "isfunction",
        "isgenerator",
        "isgeneratorfunction",
        "isgetsetdescriptor",
        "ismemberdescriptor",
        "ismethod",
        "ismethoddescriptor",
        "ismethodwrapper",
        "ismodule",
        "isroutine",
        "istraceback",
        "signature",
        "stack",
        "trace",
        "unwrap",
        "walktree",
    ]

    if sys.version_info >= (3, 12):
        __all__ += [
            "markcoroutinefunction",
            "AGEN_CLOSED",
            "AGEN_CREATED",
            "AGEN_RUNNING",
            "AGEN_SUSPENDED",
            "getasyncgenlocals",
            "getasyncgenstate",
            "BufferFlags",
        ]
    if sys.version_info >= (3, 14):
        __all__ += ["CO_HAS_DOCSTRING", "CO_METHOD", "ispackage"]

_P = ParamSpec("_P")
_T = TypeVar("_T")
_F = TypeVar("_F", bound=Callable[..., Any])
_T_contra = TypeVar("_T_contra", contravariant=True)
_V_contra = TypeVar("_V_contra", contravariant=True)

#
# Types and members
#
class EndOfBlock(Exception): ...

class BlockFinder:
    indent: int
    islambda: bool
    started: bool
    passline: bool
    indecorator: bool
    decoratorhasargs: bool
    last: int
    def tokeneater(self, type: int, token: str, srowcol: tuple[int, int], erowcol: tuple[int, int], line: str) -> None: ...

CO_OPTIMIZED: Final = 1
CO_NEWLOCALS: Final = 2
CO_VARARGS: Final = 4
CO_VARKEYWORDS: Final = 8
CO_NESTED: Final = 16
CO_GENERATOR: Final = 32
CO_NOFREE: Final = 64
CO_COROUTINE: Final = 128
CO_ITERABLE_COROUTINE: Final = 256
CO_ASYNC_GENERATOR: Final = 512
TPFLAGS_IS_ABSTRACT: Final = 1048576
if sys.version_info >= (3, 14):
    CO_HAS_DOCSTRING: Final = 67108864
    CO_METHOD: Final = 134217728

modulesbyfile: dict[str, Any]

_GetMembersPredicateTypeGuard: TypeAlias = Callable[[Any], TypeGuard[_T]]
_GetMembersPredicateTypeIs: TypeAlias = Callable[[Any], TypeIs[_T]]
_GetMembersPredicate: TypeAlias = Callable[[Any], bool]
_GetMembersReturn: TypeAlias = list[tuple[str, _T]]

@overload
def getmembers(object: object, predicate: _GetMembersPredicateTypeGuard[_T]) -> _GetMembersReturn[_T]: ...
@overload
def getmembers(object: object, predicate: _GetMembersPredicateTypeIs[_T]) -> _GetMembersReturn[_T]: ...
@overload
def getmembers(object: object, predicate: _GetMembersPredicate | None = None) -> _GetMembersReturn[Any]: ...

if sys.version_info >= (3, 11):
    @overload
    def getmembers_static(object: object, predicate: _GetMembersPredicateTypeGuard[_T]) -> _GetMembersReturn[_T]: ...
    @overload
    def getmembers_static(object: object, predicate: _GetMembersPredicateTypeIs[_T]) -> _GetMembersReturn[_T]: ...
    @overload
    def getmembers_static(object: object, predicate: _GetMembersPredicate | None = None) -> _GetMembersReturn[Any]: ...

def getmodulename(path: StrPath) -> str | None: ...
def ismodule(object: object) -> TypeIs[ModuleType]: ...
def isclass(object: object) -> TypeIs[type[Any]]: ...
def ismethod(object: object) -> TypeIs[MethodType]: ...

if sys.version_info >= (3, 14):
    # Not TypeIs because it does not return True for all modules
    def ispackage(object: object) -> TypeGuard[ModuleType]: ...

def isfunction(object: object) -> TypeIs[FunctionType]: ...

if sys.version_info >= (3, 12):
    def markcoroutinefunction(func: _F) -> _F: ...

@overload
def isgeneratorfunction(obj: Callable[..., Generator[Any, Any, Any]]) -> bool: ...
@overload
def isgeneratorfunction(obj: Callable[_P, Any]) -> TypeGuard[Callable[_P, GeneratorType[Any, Any, Any]]]: ...
@overload
def isgeneratorfunction(obj: object) -> TypeGuard[Callable[..., GeneratorType[Any, Any, Any]]]: ...
@overload
def iscoroutinefunction(obj: Callable[..., Coroutine[Any, Any, Any]]) -> bool: ...
@overload
def iscoroutinefunction(obj: Callable[_P, Awaitable[_T]]) -> TypeGuard[Callable[_P, CoroutineType[Any, Any, _T]]]: ...
@overload
def iscoroutinefunction(obj: Callable[_P, object]) -> TypeGuard[Callable[_P, CoroutineType[Any, Any, Any]]]: ...
@overload
def iscoroutinefunction(obj: object) -> TypeGuard[Callable[..., CoroutineType[Any, Any, Any]]]: ...
def isgenerator(object: object) -> TypeIs[GeneratorType[Any, Any, Any]]: ...
def iscoroutine(object: object) -> TypeIs[CoroutineType[Any, Any, Any]]: ...
def isawaitable(object: object) -> TypeIs[Awaitable[Any]]: ...
@overload
def isasyncgenfunction(obj: Callable[..., AsyncGenerator[Any, Any]]) -> bool: ...
@overload
def isasyncgenfunction(obj: Callable[_P, Any]) -> TypeGuard[Callable[_P, AsyncGeneratorType[Any, Any]]]: ...
@overload
def isasyncgenfunction(obj: object) -> TypeGuard[Callable[..., AsyncGeneratorType[Any, Any]]]: ...
@type_check_only
class _SupportsSet(Protocol[_T_contra, _V_contra]):
    def __set__(self, instance: _T_contra, value: _V_contra, /) -> None: ...

@type_check_only
class _SupportsDelete(Protocol[_T_contra]):
    def __delete__(self, instance: _T_contra, /) -> None: ...

def isasyncgen(object: object) -> TypeIs[AsyncGeneratorType[Any, Any]]: ...
def istraceback(object: object) -> TypeIs[TracebackType]: ...
def isframe(object: object) -> TypeIs[FrameType]: ...
def iscode(object: object) -> TypeIs[CodeType]: ...
def isbuiltin(object: object) -> TypeIs[BuiltinFunctionType]: ...

if sys.version_info >= (3, 11):
    def ismethodwrapper(object: object) -> TypeIs[MethodWrapperType]: ...

def isroutine(
    object: object,
) -> TypeIs[
    FunctionType
    | LambdaType
    | MethodType
    | BuiltinFunctionType
    | BuiltinMethodType
    | WrapperDescriptorType
    | MethodDescriptorType
    | ClassMethodDescriptorType
]: ...
def ismethoddescriptor(object: object) -> TypeIs[MethodDescriptorType]: ...
def ismemberdescriptor(object: object) -> TypeIs[MemberDescriptorType]: ...
def isabstract(object: object) -> bool: ...
def isgetsetdescriptor(object: object) -> TypeIs[GetSetDescriptorType]: ...
def isdatadescriptor(object: object) -> TypeIs[_SupportsSet[Any, Any] | _SupportsDelete[Any]]: ...

#
# Retrieving source code
#
_SourceObjectType: TypeAlias = (
    ModuleType | type[Any] | MethodType | FunctionType | TracebackType | FrameType | CodeType | Callable[..., Any]
)

def findsource(object: _SourceObjectType) -> tuple[list[str], int]: ...
def getabsfile(object: _SourceObjectType, _filename: str | None = None) -> str: ...

# Special-case the two most common input types here
# to avoid the annoyingly vague `Sequence[str]` return type
@overload
def getblock(lines: list[str]) -> list[str]: ...
@overload
def getblock(lines: tuple[str, ...]) -> tuple[str, ...]: ...
@overload
def getblock(lines: Sequence[str]) -> Sequence[str]: ...
def getdoc(object: object) -> str | None: ...
def getcomments(object: object) -> str | None: ...
def getfile(object: _SourceObjectType) -> str: ...
def getmodule(object: object, _filename: str | None = None) -> ModuleType | None: ...
def getsourcefile(object: _SourceObjectType) -> str | None: ...
def getsourcelines(object: _SourceObjectType) -> tuple[list[str], int]: ...
def getsource(object: _SourceObjectType) -> str: ...
def cleandoc(doc: str) -> str: ...
def indentsize(line: str) -> int: ...

_IntrospectableCallable: TypeAlias = Callable[..., Any]

#
# Introspecting callables with the Signature object
#
if sys.version_info >= (3, 14):
    def signature(
        obj: _IntrospectableCallable,
        *,
        follow_wrapped: bool = True,
        globals: Mapping[str, Any] | None = None,
        locals: Mapping[str, Any] | None = None,
        eval_str: bool = False,
        annotation_format: Format = Format.VALUE,  # noqa: Y011
    ) -> Signature: ...

elif sys.version_info >= (3, 10):
    def signature(
        obj: _IntrospectableCallable,
        *,
        follow_wrapped: bool = True,
        globals: Mapping[str, Any] | None = None,
        locals: Mapping[str, Any] | None = None,
        eval_str: bool = False,
    ) -> Signature: ...

else:
    def signature(obj: _IntrospectableCallable, *, follow_wrapped: bool = True) -> Signature: ...

class _void: ...
class _empty: ...

class Signature:
    __slots__ = ("_return_annotation", "_parameters")
    def __init__(
        self, parameters: Sequence[Parameter] | None = None, *, return_annotation: Any = ..., __validate_parameters__: bool = True
    ) -> None: ...
    empty = _empty
    @property
    def parameters(self) -> types.MappingProxyType[str, Parameter]: ...
    @property
    def return_annotation(self) -> Any: ...
    def bind(self, *args: Any, **kwargs: Any) -> BoundArguments: ...
    def bind_partial(self, *args: Any, **kwargs: Any) -> BoundArguments: ...
    def replace(self, *, parameters: Sequence[Parameter] | type[_void] | None = ..., return_annotation: Any = ...) -> Self: ...
    __replace__ = replace
    if sys.version_info >= (3, 14):
        @classmethod
        def from_callable(
            cls,
            obj: _IntrospectableCallable,
            *,
            follow_wrapped: bool = True,
            globals: Mapping[str, Any] | None = None,
            locals: Mapping[str, Any] | None = None,
            eval_str: bool = False,
            annotation_format: Format = Format.VALUE,  # noqa: Y011
        ) -> Self: ...
    elif sys.version_info >= (3, 10):
        @classmethod
        def from_callable(
            cls,
            obj: _IntrospectableCallable,
            *,
            follow_wrapped: bool = True,
            globals: Mapping[str, Any] | None = None,
            locals: Mapping[str, Any] | None = None,
            eval_str: bool = False,
        ) -> Self: ...
    else:
        @classmethod
        def from_callable(cls, obj: _IntrospectableCallable, *, follow_wrapped: bool = True) -> Self: ...
    if sys.version_info >= (3, 14):
        def format(self, *, max_width: int | None = None, quote_annotation_strings: bool = True) -> str: ...
    elif sys.version_info >= (3, 13):
        def format(self, *, max_width: int | None = None) -> str: ...

    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

if sys.version_info >= (3, 14):
    from annotationlib import get_annotations as get_annotations
elif sys.version_info >= (3, 10):
    def get_annotations(
        obj: Callable[..., object] | type[object] | ModuleType,  # any callable, class, or module
        *,
        globals: Mapping[str, Any] | None = None,  # value types depend on the key
        locals: Mapping[str, Any] | None = None,  # value types depend on the key
        eval_str: bool = False,
    ) -> dict[str, AnnotationForm]: ...  # values are type expressions

# The name is the same as the enum's name in CPython
class _ParameterKind(enum.IntEnum):
    POSITIONAL_ONLY = 0
    POSITIONAL_OR_KEYWORD = 1
    VAR_POSITIONAL = 2
    KEYWORD_ONLY = 3
    VAR_KEYWORD = 4

    @property
    def description(self) -> str: ...

if sys.version_info >= (3, 12):
    AGEN_CREATED: Final = "AGEN_CREATED"
    AGEN_RUNNING: Final = "AGEN_RUNNING"
    AGEN_SUSPENDED: Final = "AGEN_SUSPENDED"
    AGEN_CLOSED: Final = "AGEN_CLOSED"

    def getasyncgenstate(
        agen: AsyncGenerator[Any, Any],
    ) -> Literal["AGEN_CREATED", "AGEN_RUNNING", "AGEN_SUSPENDED", "AGEN_CLOSED"]: ...
    def getasyncgenlocals(agen: AsyncGeneratorType[Any, Any]) -> dict[str, Any]: ...

class Parameter:
    __slots__ = ("_name", "_kind", "_default", "_annotation")
    def __init__(self, name: str, kind: _ParameterKind, *, default: Any = ..., annotation: Any = ...) -> None: ...
    empty = _empty

    POSITIONAL_ONLY: ClassVar[Literal[_ParameterKind.POSITIONAL_ONLY]]
    POSITIONAL_OR_KEYWORD: ClassVar[Literal[_ParameterKind.POSITIONAL_OR_KEYWORD]]
    VAR_POSITIONAL: ClassVar[Literal[_ParameterKind.VAR_POSITIONAL]]
    KEYWORD_ONLY: ClassVar[Literal[_ParameterKind.KEYWORD_ONLY]]
    VAR_KEYWORD: ClassVar[Literal[_ParameterKind.VAR_KEYWORD]]
    @property
    def name(self) -> str: ...
    @property
    def default(self) -> Any: ...
    @property
    def kind(self) -> _ParameterKind: ...
    @property
    def annotation(self) -> Any: ...
    def replace(
        self,
        *,
        name: str | type[_void] = ...,
        kind: _ParameterKind | type[_void] = ...,
        default: Any = ...,
        annotation: Any = ...,
    ) -> Self: ...
    if sys.version_info >= (3, 13):
        __replace__ = replace

    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

class BoundArguments:
    __slots__ = ("arguments", "_signature", "__weakref__")
    arguments: OrderedDict[str, Any]
    @property
    def args(self) -> tuple[Any, ...]: ...
    @property
    def kwargs(self) -> dict[str, Any]: ...
    @property
    def signature(self) -> Signature: ...
    def __init__(self, signature: Signature, arguments: OrderedDict[str, Any]) -> None: ...
    def apply_defaults(self) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]

#
# Classes and functions
#

_ClassTreeItem: TypeAlias = list[tuple[type, ...]] | list[_ClassTreeItem]

def getclasstree(classes: list[type], unique: bool = False) -> _ClassTreeItem: ...
def walktree(classes: list[type], children: Mapping[type[Any], list[type]], parent: type[Any] | None) -> _ClassTreeItem: ...

class Arguments(NamedTuple):
    args: list[str]
    varargs: str | None
    varkw: str | None

def getargs(co: CodeType) -> Arguments: ...

if sys.version_info < (3, 11):
    @deprecated("Deprecated since Python 3.0; removed in Python 3.11.")
    class ArgSpec(NamedTuple):
        args: list[str]
        varargs: str | None
        keywords: str | None
        defaults: tuple[Any, ...]

    @deprecated("Deprecated since Python 3.0; removed in Python 3.11. Use `inspect.signature()` instead.")
    def getargspec(func: object) -> ArgSpec: ...

class FullArgSpec(NamedTuple):
    args: list[str]
    varargs: str | None
    varkw: str | None
    defaults: tuple[Any, ...] | None
    kwonlyargs: list[str]
    kwonlydefaults: dict[str, Any] | None
    annotations: dict[str, Any]

def getfullargspec(func: object) -> FullArgSpec: ...

class ArgInfo(NamedTuple):
    args: list[str]
    varargs: str | None
    keywords: str | None
    locals: dict[str, Any]

def getargvalues(frame: FrameType) -> ArgInfo: ...

if sys.version_info >= (3, 14):
    def formatannotation(annotation: object, base_module: str | None = None, *, quote_annotation_strings: bool = True) -> str: ...

else:
    def formatannotation(annotation: object, base_module: str | None = None) -> str: ...

def formatannotationrelativeto(object: object) -> Callable[[object], str]: ...

if sys.version_info < (3, 11):
    @deprecated(
        "Deprecated since Python 3.5; removed in Python 3.11. Use `inspect.signature()` and the `Signature` class instead."
    )
    def formatargspec(
        args: list[str],
        varargs: str | None = None,
        varkw: str | None = None,
        defaults: tuple[Any, ...] | None = None,
        kwonlyargs: Sequence[str] | None = (),
        kwonlydefaults: Mapping[str, Any] | None = {},
        annotations: Mapping[str, Any] = {},
        formatarg: Callable[[str], str] = ...,
        formatvarargs: Callable[[str], str] = ...,
        formatvarkw: Callable[[str], str] = ...,
        formatvalue: Callable[[Any], str] = ...,
        formatreturns: Callable[[Any], str] = ...,
        formatannotation: Callable[[Any], str] = ...,
    ) -> str: ...

def formatargvalues(
    args: list[str],
    varargs: str | None,
    varkw: str | None,
    locals: Mapping[str, Any] | None,
    formatarg: Callable[[str], str] | None = ...,
    formatvarargs: Callable[[str], str] | None = ...,
    formatvarkw: Callable[[str], str] | None = ...,
    formatvalue: Callable[[Any], str] | None = ...,
) -> str: ...
def getmro(cls: type) -> tuple[type, ...]: ...
def getcallargs(func: Callable[_P, Any], /, *args: _P.args, **kwds: _P.kwargs) -> dict[str, Any]: ...

class ClosureVars(NamedTuple):
    nonlocals: Mapping[str, Any]
    globals: Mapping[str, Any]
    builtins: Mapping[str, Any]
    unbound: AbstractSet[str]

def getclosurevars(func: _IntrospectableCallable) -> ClosureVars: ...
def unwrap(func: Callable[..., Any], *, stop: Callable[[Callable[..., Any]], Any] | None = None) -> Any: ...

#
# The interpreter stack
#

if sys.version_info >= (3, 11):
    class _Traceback(NamedTuple):
        filename: str
        lineno: int
        function: str
        code_context: list[str] | None
        index: int | None  # type: ignore[assignment]

    class _FrameInfo(NamedTuple):
        frame: FrameType
        filename: str
        lineno: int
        function: str
        code_context: list[str] | None
        index: int | None  # type: ignore[assignment]

    if sys.version_info >= (3, 12):
        class Traceback(_Traceback):
            positions: dis.Positions | None
            def __new__(
                cls,
                filename: str,
                lineno: int,
                function: str,
                code_context: list[str] | None,
                index: int | None,
                *,
                positions: dis.Positions | None = None,
            ) -> Self: ...

        class FrameInfo(_FrameInfo):
            positions: dis.Positions | None
            def __new__(
                cls,
                frame: FrameType,
                filename: str,
                lineno: int,
                function: str,
                code_context: list[str] | None,
                index: int | None,
                *,
                positions: dis.Positions | None = None,
            ) -> Self: ...

    else:
        @disjoint_base
        class Traceback(_Traceback):
            positions: dis.Positions | None
            def __new__(
                cls,
                filename: str,
                lineno: int,
                function: str,
                code_context: list[str] | None,
                index: int | None,
                *,
                positions: dis.Positions | None = None,
            ) -> Self: ...

        @disjoint_base
        class FrameInfo(_FrameInfo):
            positions: dis.Positions | None
            def __new__(
                cls,
                frame: FrameType,
                filename: str,
                lineno: int,
                function: str,
                code_context: list[str] | None,
                index: int | None,
                *,
                positions: dis.Positions | None = None,
            ) -> Self: ...

else:
    class Traceback(NamedTuple):
        filename: str
        lineno: int
        function: str
        code_context: list[str] | None
        index: int | None  # type: ignore[assignment]

    class FrameInfo(NamedTuple):
        frame: FrameType
        filename: str
        lineno: int
        function: str
        code_context: list[str] | None
        index: int | None  # type: ignore[assignment]

def getframeinfo(frame: FrameType | TracebackType, context: int = 1) -> Traceback: ...
def getouterframes(frame: Any, context: int = 1) -> list[FrameInfo]: ...
def getinnerframes(tb: TracebackType, context: int = 1) -> list[FrameInfo]: ...
def getlineno(frame: FrameType) -> int: ...
def currentframe() -> FrameType | None: ...
def stack(context: int = 1) -> list[FrameInfo]: ...
def trace(context: int = 1) -> list[FrameInfo]: ...

#
# Fetching attributes statically
#

def getattr_static(obj: object, attr: str, default: Any | None = ...) -> Any: ...

#
# Current State of Generators and Coroutines
#

GEN_CREATED: Final = "GEN_CREATED"
GEN_RUNNING: Final = "GEN_RUNNING"
GEN_SUSPENDED: Final = "GEN_SUSPENDED"
GEN_CLOSED: Final = "GEN_CLOSED"

def getgeneratorstate(
    generator: Generator[Any, Any, Any],
) -> Literal["GEN_CREATED", "GEN_RUNNING", "GEN_SUSPENDED", "GEN_CLOSED"]: ...

CORO_CREATED: Final = "CORO_CREATED"
CORO_RUNNING: Final = "CORO_RUNNING"
CORO_SUSPENDED: Final = "CORO_SUSPENDED"
CORO_CLOSED: Final = "CORO_CLOSED"

def getcoroutinestate(
    coroutine: Coroutine[Any, Any, Any],
) -> Literal["CORO_CREATED", "CORO_RUNNING", "CORO_SUSPENDED", "CORO_CLOSED"]: ...
def getgeneratorlocals(generator: Generator[Any, Any, Any]) -> dict[str, Any]: ...
def getcoroutinelocals(coroutine: Coroutine[Any, Any, Any]) -> dict[str, Any]: ...

# Create private type alias to avoid conflict with symbol of same
# name created in Attribute class.
_Object: TypeAlias = object

class Attribute(NamedTuple):
    name: str
    kind: Literal["class method", "static method", "property", "method", "data"]
    defining_class: type
    object: _Object

def classify_class_attrs(cls: type) -> list[Attribute]: ...

class ClassFoundException(Exception): ...

if sys.version_info >= (3, 12):
    class BufferFlags(enum.IntFlag):
        SIMPLE = 0
        WRITABLE = 1
        FORMAT = 4
        ND = 8
        STRIDES = 24
        C_CONTIGUOUS = 56
        F_CONTIGUOUS = 88
        ANY_CONTIGUOUS = 152
        INDIRECT = 280
        CONTIG = 9
        CONTIG_RO = 8
        STRIDED = 25
        STRIDED_RO = 24
        RECORDS = 29
        RECORDS_RO = 28
        FULL = 285
        FULL_RO = 284
        READ = 256
        WRITE = 512
