"""Get useful information from live Python objects.

This module encapsulates the interface provided by the internal special
attributes (co_*, im_*, tb_*, etc.) in a friendlier fashion.
It also provides some help for examining source code and class layout.

Here are some of the useful functions provided by this module:

    ismodule(), isclass(), ismethod(), ispackage(), isfunction(),
        isgeneratorfunction(), isgenerator(), istraceback(), isframe(),
        iscode(), isbuiltin(), isroutine() - check object types
    getmembers() - get members of an object that satisfy a given condition

    getfile(), getsourcefile(), getsource() - find an object's source code
    getdoc(), getcomments() - get documentation on an object
    getmodule() - determine the module that an object came from
    getclasstree() - arrange classes so as to represent their hierarchy

    getargvalues(), getcallargs() - get info about function arguments
    getfullargspec() - same, with support for Python 3 features
    formatargvalues() - format an argument spec
    getouterframes(), getinnerframes() - get info about frames
    currentframe() - get the current stack frame
    stack(), trace() - get info about frames on the stack or in a traceback

    signature() - get a Signature object for the callable
"""

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
    """Provide a tokeneater() method to detect the end of a code block."""

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
def getmembers(object: object, predicate: _GetMembersPredicateTypeGuard[_T]) -> _GetMembersReturn[_T]:
    """Return all members of an object as (name, value) pairs sorted by name.
    Optionally, only return members that satisfy a given predicate.
    """

@overload
def getmembers(object: object, predicate: _GetMembersPredicateTypeIs[_T]) -> _GetMembersReturn[_T]: ...
@overload
def getmembers(object: object, predicate: _GetMembersPredicate | None = None) -> _GetMembersReturn[Any]: ...

if sys.version_info >= (3, 11):
    @overload
    def getmembers_static(object: object, predicate: _GetMembersPredicateTypeGuard[_T]) -> _GetMembersReturn[_T]:
        """Return all members of an object as (name, value) pairs sorted by name
        without triggering dynamic lookup via the descriptor protocol,
        __getattr__ or __getattribute__. Optionally, only return members that
        satisfy a given predicate.

        Note: this function may not be able to retrieve all members
           that getmembers can fetch (like dynamically created attributes)
           and may find members that getmembers can't (like descriptors
           that raise AttributeError). It can also return descriptor objects
           instead of instance members in some cases.
        """

    @overload
    def getmembers_static(object: object, predicate: _GetMembersPredicateTypeIs[_T]) -> _GetMembersReturn[_T]: ...
    @overload
    def getmembers_static(object: object, predicate: _GetMembersPredicate | None = None) -> _GetMembersReturn[Any]: ...

def getmodulename(path: StrPath) -> str | None:
    """Return the module name for a given file, or None."""

def ismodule(object: object) -> TypeIs[ModuleType]:
    """Return true if the object is a module."""

def isclass(object: object) -> TypeIs[type[Any]]:
    """Return true if the object is a class."""

def ismethod(object: object) -> TypeIs[MethodType]:
    """Return true if the object is an instance method."""

if sys.version_info >= (3, 14):
    # Not TypeIs because it does not return True for all modules
    def ispackage(object: object) -> TypeGuard[ModuleType]:
        """Return true if the object is a package."""

def isfunction(object: object) -> TypeIs[FunctionType]:
    """Return true if the object is a user-defined function.

    Function objects provide these attributes:
        __doc__         documentation string
        __name__        name with which this function was defined
        __qualname__    qualified name of this function
        __module__      name of the module the function was defined in or None
        __code__        code object containing compiled function bytecode
        __defaults__    tuple of any default values for arguments
        __globals__     global namespace in which this function was defined
        __annotations__ dict of parameter annotations
        __kwdefaults__  dict of keyword only parameters with defaults
        __dict__        namespace which is supporting arbitrary function attributes
        __closure__     a tuple of cells or None
        __type_params__ tuple of type parameters
    """

if sys.version_info >= (3, 12):
    def markcoroutinefunction(func: _F) -> _F:
        """
        Decorator to ensure callable is recognised as a coroutine function.
        """

@overload
def isgeneratorfunction(obj: Callable[..., Generator[Any, Any, Any]]) -> bool:
    """Return true if the object is a user-defined generator function.

    Generator function objects provide the same attributes as functions.
    See help(isfunction) for a list of attributes.
    """

@overload
def isgeneratorfunction(obj: Callable[_P, Any]) -> TypeGuard[Callable[_P, GeneratorType[Any, Any, Any]]]: ...
@overload
def isgeneratorfunction(obj: object) -> TypeGuard[Callable[..., GeneratorType[Any, Any, Any]]]: ...
@overload
def iscoroutinefunction(obj: Callable[..., Coroutine[Any, Any, Any]]) -> bool:
    """Return true if the object is a coroutine function.

    Coroutine functions are normally defined with "async def" syntax, but may
    be marked via markcoroutinefunction.
    """

@overload
def iscoroutinefunction(obj: Callable[_P, Awaitable[_T]]) -> TypeGuard[Callable[_P, CoroutineType[Any, Any, _T]]]: ...
@overload
def iscoroutinefunction(obj: Callable[_P, object]) -> TypeGuard[Callable[_P, CoroutineType[Any, Any, Any]]]: ...
@overload
def iscoroutinefunction(obj: object) -> TypeGuard[Callable[..., CoroutineType[Any, Any, Any]]]: ...
def isgenerator(object: object) -> TypeIs[GeneratorType[Any, Any, Any]]:
    """Return true if the object is a generator.

    Generator objects provide these attributes:
        gi_code         code object
        gi_frame        frame object or possibly None once the generator has
                        been exhausted
        gi_running      set to 1 when generator is executing, 0 otherwise
        gi_yieldfrom    object being iterated by yield from or None

        __iter__()      defined to support iteration over container
        close()         raises a new GeneratorExit exception inside the
                        generator to terminate the iteration
        send()          resumes the generator and "sends" a value that becomes
                        the result of the current yield-expression
        throw()         used to raise an exception inside the generator
    """

def iscoroutine(object: object) -> TypeIs[CoroutineType[Any, Any, Any]]:
    """Return true if the object is a coroutine."""

def isawaitable(object: object) -> TypeIs[Awaitable[Any]]:
    """Return true if object can be passed to an ``await`` expression."""

@overload
def isasyncgenfunction(obj: Callable[..., AsyncGenerator[Any, Any]]) -> bool:
    """Return true if the object is an asynchronous generator function.

    Asynchronous generator functions are defined with "async def"
    syntax and have "yield" expressions in their body.
    """

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

def isasyncgen(object: object) -> TypeIs[AsyncGeneratorType[Any, Any]]:
    """Return true if the object is an asynchronous generator."""

def istraceback(object: object) -> TypeIs[TracebackType]:
    """Return true if the object is a traceback.

    Traceback objects provide these attributes:
        tb_frame        frame object at this level
        tb_lasti        index of last attempted instruction in bytecode
        tb_lineno       current line number in Python source code
        tb_next         next inner traceback object (called by this level)
    """

def isframe(object: object) -> TypeIs[FrameType]:
    """Return true if the object is a frame object.

    Frame objects provide these attributes:
        f_back          next outer frame object (this frame's caller)
        f_builtins      built-in namespace seen by this frame
        f_code          code object being executed in this frame
        f_globals       global namespace seen by this frame
        f_lasti         index of last attempted instruction in bytecode
        f_lineno        current line number in Python source code
        f_locals        local namespace seen by this frame
        f_trace         tracing function for this frame, or None
        f_trace_lines   is a tracing event triggered for each source line?
        f_trace_opcodes are per-opcode events being requested?

        clear()          used to clear all references to local variables
    """

def iscode(object: object) -> TypeIs[CodeType]:
    """Return true if the object is a code object.

    Code objects provide these attributes:
        co_argcount         number of arguments (not including *, ** args
                            or keyword only arguments)
        co_code             string of raw compiled bytecode
        co_cellvars         tuple of names of cell variables
        co_consts           tuple of constants used in the bytecode
        co_filename         name of file in which this code object was created
        co_firstlineno      number of first line in Python source code
        co_flags            bitmap: 1=optimized | 2=newlocals | 4=*arg | 8=**arg
                            | 16=nested | 32=generator | 64=nofree | 128=coroutine
                            | 256=iterable_coroutine | 512=async_generator
                            | 0x4000000=has_docstring
        co_freevars         tuple of names of free variables
        co_posonlyargcount  number of positional only arguments
        co_kwonlyargcount   number of keyword only arguments (not including ** arg)
        co_lnotab           encoded mapping of line numbers to bytecode indices
        co_name             name with which this code object was defined
        co_names            tuple of names other than arguments and function locals
        co_nlocals          number of local variables
        co_stacksize        virtual machine stack space required
        co_varnames         tuple of names of arguments and local variables
        co_qualname         fully qualified function name

        co_lines()          returns an iterator that yields successive bytecode ranges
        co_positions()      returns an iterator of source code positions for each bytecode instruction
        replace()           returns a copy of the code object with a new values
    """

def isbuiltin(object: object) -> TypeIs[BuiltinFunctionType]:
    """Return true if the object is a built-in function or method.

    Built-in functions and methods provide these attributes:
        __doc__         documentation string
        __name__        original name of this function or method
        __self__        instance to which a method is bound, or None
    """

if sys.version_info >= (3, 11):
    def ismethodwrapper(object: object) -> TypeIs[MethodWrapperType]:
        """Return true if the object is a method wrapper."""

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
]:
    """Return true if the object is any kind of function or method."""

def ismethoddescriptor(object: object) -> TypeIs[MethodDescriptorType]:
    """Return true if the object is a method descriptor.

    But not if ismethod() or isclass() or isfunction() are true.

    This is new in Python 2.2, and, for example, is true of int.__add__.
    An object passing this test has a __get__ attribute, but not a
    __set__ attribute or a __delete__ attribute. Beyond that, the set
    of attributes varies; __name__ is usually sensible, and __doc__
    often is.

    Methods implemented via descriptors that also pass one of the other
    tests return false from the ismethoddescriptor() test, simply because
    the other tests promise more -- you can, e.g., count on having the
    __func__ attribute (etc) when an object passes ismethod().
    """

def ismemberdescriptor(object: object) -> TypeIs[MemberDescriptorType]:
    """Return true if the object is a member descriptor.

    Member descriptors are specialized descriptors defined in extension
    modules.
    """

def isabstract(object: object) -> bool:
    """Return true if the object is an abstract base class (ABC)."""

def isgetsetdescriptor(object: object) -> TypeIs[GetSetDescriptorType]:
    """Return true if the object is a getset descriptor.

    getset descriptors are specialized descriptors defined in extension
    modules.
    """

def isdatadescriptor(object: object) -> TypeIs[_SupportsSet[Any, Any] | _SupportsDelete[Any]]:
    """Return true if the object is a data descriptor.

    Data descriptors have a __set__ or a __delete__ attribute.  Examples are
    properties (defined in Python) and getsets and members (defined in C).
    Typically, data descriptors will also have __name__ and __doc__ attributes
    (properties, getsets, and members have both of these attributes), but this
    is not guaranteed.
    """

#
# Retrieving source code
#
_SourceObjectType: TypeAlias = (
    ModuleType | type[Any] | MethodType | FunctionType | TracebackType | FrameType | CodeType | Callable[..., Any]
)

def findsource(object: _SourceObjectType) -> tuple[list[str], int]:
    """Return the entire source file and starting line number for an object.

    The argument may be a module, class, method, function, traceback, frame,
    or code object.  The source code is returned as a list of all the lines
    in the file and the line number indexes a line in that list.  An OSError
    is raised if the source code cannot be retrieved.
    """

def getabsfile(object: _SourceObjectType, _filename: str | None = None) -> str:
    """Return an absolute path to the source or compiled file for an object.

    The idea is for each object to have a unique origin, so this routine
    normalizes the result as much as possible.
    """

# Special-case the two most common input types here
# to avoid the annoyingly vague `Sequence[str]` return type
@overload
def getblock(lines: list[str]) -> list[str]:
    """Extract the block of code at the top of the given list of lines."""

@overload
def getblock(lines: tuple[str, ...]) -> tuple[str, ...]: ...
@overload
def getblock(lines: Sequence[str]) -> Sequence[str]: ...
def getdoc(object: object) -> str | None:
    """Get the documentation string for an object.

    All tabs are expanded to spaces.  To clean up docstrings that are
    indented to line up with blocks of code, any whitespace than can be
    uniformly removed from the second line onwards is removed.
    """

def getcomments(object: object) -> str | None:
    """Get lines of comments immediately preceding an object's source code.

    Returns None when source can't be found.
    """

def getfile(object: _SourceObjectType) -> str:
    """Work out which source or compiled file an object was defined in."""

def getmodule(object: object, _filename: str | None = None) -> ModuleType | None:
    """Return the module an object was defined in, or None if not found."""

def getsourcefile(object: _SourceObjectType) -> str | None:
    """Return the filename that can be used to locate an object's source.
    Return None if no way can be identified to get the source.
    """

def getsourcelines(object: _SourceObjectType) -> tuple[list[str], int]:
    """Return a list of source lines and starting line number for an object.

    The argument may be a module, class, method, function, traceback, frame,
    or code object.  The source code is returned as a list of the lines
    corresponding to the object and the line number indicates where in the
    original source file the first line of code was found.  An OSError is
    raised if the source code cannot be retrieved.
    """

def getsource(object: _SourceObjectType) -> str:
    """Return the text of the source code for an object.

    The argument may be a module, class, method, function, traceback, frame,
    or code object.  The source code is returned as a single string.  An
    OSError is raised if the source code cannot be retrieved.
    """

def cleandoc(doc: str) -> str:
    """Clean up indentation from docstrings.

    Any whitespace that can be uniformly removed from the second line
    onwards is removed.
    """

def indentsize(line: str) -> int:
    """Return the indent size, in spaces, at the start of a line of text."""

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
    ) -> Signature:
        """Get a signature object for the passed callable."""

elif sys.version_info >= (3, 10):
    def signature(
        obj: _IntrospectableCallable,
        *,
        follow_wrapped: bool = True,
        globals: Mapping[str, Any] | None = None,
        locals: Mapping[str, Any] | None = None,
        eval_str: bool = False,
    ) -> Signature:
        """Get a signature object for the passed callable."""

else:
    def signature(obj: _IntrospectableCallable, *, follow_wrapped: bool = True) -> Signature:
        """Get a signature object for the passed callable."""

class _void:
    """A private marker - used in Parameter & Signature."""

class _empty:
    """Marker object for Signature.empty and Parameter.empty."""

class Signature:
    """A Signature object represents the overall signature of a function.
    It stores a Parameter object for each parameter accepted by the
    function, as well as information specific to the function itself.

    A Signature object has the following public attributes and methods:

    * parameters : OrderedDict
        An ordered mapping of parameters' names to the corresponding
        Parameter objects (keyword-only arguments are in the same order
        as listed in `code.co_varnames`).
    * return_annotation : object
        The annotation for the return type of the function if specified.
        If the function has no annotation for its return type, this
        attribute is set to `Signature.empty`.
    * bind(*args, **kwargs) -> BoundArguments
        Creates a mapping from positional and keyword arguments to
        parameters.
    * bind_partial(*args, **kwargs) -> BoundArguments
        Creates a partial mapping from positional and keyword arguments
        to parameters (simulating 'functools.partial' behavior.)
    """

    __slots__ = ("_return_annotation", "_parameters")
    def __init__(
        self, parameters: Sequence[Parameter] | None = None, *, return_annotation: Any = ..., __validate_parameters__: bool = True
    ) -> None:
        """Constructs Signature from the given list of Parameter
        objects and 'return_annotation'.  All arguments are optional.
        """
    empty = _empty
    @property
    def parameters(self) -> types.MappingProxyType[str, Parameter]: ...
    @property
    def return_annotation(self) -> Any: ...
    def bind(self, *args: Any, **kwargs: Any) -> BoundArguments:
        """Get a BoundArguments object, that maps the passed `args`
        and `kwargs` to the function's signature.  Raises `TypeError`
        if the passed arguments can not be bound.
        """

    def bind_partial(self, *args: Any, **kwargs: Any) -> BoundArguments:
        """Get a BoundArguments object, that partially maps the
        passed `args` and `kwargs` to the function's signature.
        Raises `TypeError` if the passed arguments can not be bound.
        """

    def replace(self, *, parameters: Sequence[Parameter] | type[_void] | None = ..., return_annotation: Any = ...) -> Self:
        """Creates a customized copy of the Signature.
        Pass 'parameters' and/or 'return_annotation' arguments
        to override them in the new copy.
        """
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
        ) -> Self:
            """Constructs Signature for the given callable object."""
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
        ) -> Self:
            """Constructs Signature for the given callable object."""
    else:
        @classmethod
        def from_callable(cls, obj: _IntrospectableCallable, *, follow_wrapped: bool = True) -> Self:
            """Constructs Signature for the given callable object."""
    if sys.version_info >= (3, 14):
        def format(self, *, max_width: int | None = None, quote_annotation_strings: bool = True) -> str:
            """Create a string representation of the Signature object.

            If *max_width* integer is passed,
            signature will try to fit into the *max_width*.
            If signature is longer than *max_width*,
            all parameters will be on separate lines.

            If *quote_annotation_strings* is False, annotations
            in the signature are displayed without opening and closing quotation
            marks. This is useful when the signature was created with the
            STRING format or when ``from __future__ import annotations`` was used.
            """
    elif sys.version_info >= (3, 13):
        def format(self, *, max_width: int | None = None) -> str:
            """Create a string representation of the Signature object.

            If *max_width* integer is passed,
            signature will try to fit into the *max_width*.
            If signature is longer than *max_width*,
            all parameters will be on separate lines.
            """

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
    ) -> dict[str, AnnotationForm]:  # values are type expressions
        """Compute the annotations dict for an object.

        obj may be a callable, class, or module.
        Passing in an object of any other type raises TypeError.

        Returns a dict.  get_annotations() returns a new dict every time
        it's called; calling it twice on the same object will return two
        different but equivalent dicts.

        This function handles several details for you:

          * If eval_str is true, values of type str will
            be un-stringized using eval().  This is intended
            for use with stringized annotations
            ("from __future__ import annotations").
          * If obj doesn't have an annotations dict, returns an
            empty dict.  (Functions and methods always have an
            annotations dict; classes, modules, and other types of
            callables may not.)
          * Ignores inherited annotations on classes.  If a class
            doesn't have its own annotations dict, returns an empty dict.
          * All accesses to object members and dict values are done
            using getattr() and dict.get() for safety.
          * Always, always, always returns a freshly-created dict.

        eval_str controls whether or not values of type str are replaced
        with the result of calling eval() on those values:

          * If eval_str is true, eval() is called on values of type str.
          * If eval_str is false (the default), values of type str are unchanged.

        globals and locals are passed in to eval(); see the documentation
        for eval() for more information.  If either globals or locals is
        None, this function may replace that value with a context-specific
        default, contingent on type(obj):

          * If obj is a module, globals defaults to obj.__dict__.
          * If obj is a class, globals defaults to
            sys.modules[obj.__module__].__dict__ and locals
            defaults to the obj class namespace.
          * If obj is a callable, globals defaults to obj.__globals__,
            although if obj is a wrapped function (using
            functools.update_wrapper()) it is first unwrapped.
        """

# The name is the same as the enum's name in CPython
class _ParameterKind(enum.IntEnum):
    """An enumeration."""

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
    ) -> Literal["AGEN_CREATED", "AGEN_RUNNING", "AGEN_SUSPENDED", "AGEN_CLOSED"]:
        """Get current state of an asynchronous generator object.

        Possible states are:
          AGEN_CREATED: Waiting to start execution.
          AGEN_RUNNING: Currently being executed by the interpreter.
          AGEN_SUSPENDED: Currently suspended at a yield expression.
          AGEN_CLOSED: Execution has completed.
        """

    def getasyncgenlocals(agen: AsyncGeneratorType[Any, Any]) -> dict[str, Any]:
        """
        Get the mapping of asynchronous generator local variables to their current
        values.

        A dict is returned, with the keys the local variable names and values the
        bound values.
        """

class Parameter:
    """Represents a parameter in a function signature.

    Has the following public attributes:

    * name : str
        The name of the parameter as a string.
    * default : object
        The default value for the parameter if specified.  If the
        parameter has no default value, this attribute is set to
        `Parameter.empty`.
    * annotation
        The annotation for the parameter if specified.  If the
        parameter has no annotation, this attribute is set to
        `Parameter.empty`.
    * kind : str
        Describes how argument values are bound to the parameter.
        Possible values: `Parameter.POSITIONAL_ONLY`,
        `Parameter.POSITIONAL_OR_KEYWORD`, `Parameter.VAR_POSITIONAL`,
        `Parameter.KEYWORD_ONLY`, `Parameter.VAR_KEYWORD`.
    """

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
    ) -> Self:
        """Creates a customized copy of the Parameter."""
    if sys.version_info >= (3, 13):
        __replace__ = replace

    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

class BoundArguments:
    """Result of `Signature.bind` call.  Holds the mapping of arguments
    to the function's parameters.

    Has the following public attributes:

    * arguments : dict
        An ordered mutable mapping of parameters' names to arguments' values.
        Does not contain arguments' default values.
    * signature : Signature
        The Signature object that created this instance.
    * args : tuple
        Tuple of positional arguments values.
    * kwargs : dict
        Dict of keyword arguments values.
    """

    __slots__ = ("arguments", "_signature", "__weakref__")
    arguments: OrderedDict[str, Any]
    @property
    def args(self) -> tuple[Any, ...]: ...
    @property
    def kwargs(self) -> dict[str, Any]: ...
    @property
    def signature(self) -> Signature: ...
    def __init__(self, signature: Signature, arguments: OrderedDict[str, Any]) -> None: ...
    def apply_defaults(self) -> None:
        """Set default values for missing arguments.

        For variable-positional arguments (*args) the default is an
        empty tuple.

        For variable-keyword arguments (**kwargs) the default is an
        empty dict.
        """

    def __eq__(self, other: object) -> bool: ...
    __hash__: ClassVar[None]  # type: ignore[assignment]

#
# Classes and functions
#

_ClassTreeItem: TypeAlias = list[tuple[type, ...]] | list[_ClassTreeItem]

def getclasstree(classes: list[type], unique: bool = False) -> _ClassTreeItem:
    """Arrange the given list of classes into a hierarchy of nested lists.

    Where a nested list appears, it contains classes derived from the class
    whose entry immediately precedes the list.  Each entry is a 2-tuple
    containing a class and a tuple of its base classes.  If the 'unique'
    argument is true, exactly one entry appears in the returned structure
    for each class in the given list.  Otherwise, classes using multiple
    inheritance and their descendants will appear multiple times.
    """

def walktree(classes: list[type], children: Mapping[type[Any], list[type]], parent: type[Any] | None) -> _ClassTreeItem:
    """Recursive helper function for getclasstree()."""

class Arguments(NamedTuple):
    """Arguments(args, varargs, varkw)"""

    args: list[str]
    varargs: str | None
    varkw: str | None

def getargs(co: CodeType) -> Arguments:
    """Get information about the arguments accepted by a code object.

    Three things are returned: (args, varargs, varkw), where
    'args' is the list of argument names. Keyword-only arguments are
    appended. 'varargs' and 'varkw' are the names of the * and **
    arguments or None.
    """

if sys.version_info < (3, 11):
    @deprecated("Deprecated since Python 3.0; removed in Python 3.11.")
    class ArgSpec(NamedTuple):
        """ArgSpec(args, varargs, keywords, defaults)"""

        args: list[str]
        varargs: str | None
        keywords: str | None
        defaults: tuple[Any, ...]

    @deprecated("Deprecated since Python 3.0; removed in Python 3.11. Use `inspect.signature()` instead.")
    def getargspec(func: object) -> ArgSpec:
        """Get the names and default values of a function's parameters.

        A tuple of four things is returned: (args, varargs, keywords, defaults).
        'args' is a list of the argument names, including keyword-only argument names.
        'varargs' and 'keywords' are the names of the * and ** parameters or None.
        'defaults' is an n-tuple of the default values of the last n parameters.

        This function is deprecated, as it does not support annotations or
        keyword-only parameters and will raise ValueError if either is present
        on the supplied callable.

        For a more structured introspection API, use inspect.signature() instead.

        Alternatively, use getfullargspec() for an API with a similar namedtuple
        based interface, but full support for annotations and keyword-only
        parameters.

        Deprecated since Python 3.5, use `inspect.getfullargspec()`.
        """

class FullArgSpec(NamedTuple):
    """FullArgSpec(args, varargs, varkw, defaults, kwonlyargs, kwonlydefaults, annotations)"""

    args: list[str]
    varargs: str | None
    varkw: str | None
    defaults: tuple[Any, ...] | None
    kwonlyargs: list[str]
    kwonlydefaults: dict[str, Any] | None
    annotations: dict[str, Any]

def getfullargspec(func: object) -> FullArgSpec:
    """Get the names and default values of a callable object's parameters.

    A tuple of seven things is returned:
    (args, varargs, varkw, defaults, kwonlyargs, kwonlydefaults, annotations).
    'args' is a list of the parameter names.
    'varargs' and 'varkw' are the names of the * and ** parameters or None.
    'defaults' is an n-tuple of the default values of the last n parameters.
    'kwonlyargs' is a list of keyword-only parameter names.
    'kwonlydefaults' is a dictionary mapping names from kwonlyargs to defaults.
    'annotations' is a dictionary mapping parameter names to annotations.

    Notable differences from inspect.signature():
      - the "self" parameter is always reported, even for bound methods
      - wrapper chains defined by __wrapped__ *not* unwrapped automatically
    """

class ArgInfo(NamedTuple):
    """ArgInfo(args, varargs, keywords, locals)"""

    args: list[str]
    varargs: str | None
    keywords: str | None
    locals: dict[str, Any]

def getargvalues(frame: FrameType) -> ArgInfo:
    """Get information about arguments passed into a particular frame.

    A tuple of four things is returned: (args, varargs, varkw, locals).
    'args' is a list of the argument names.
    'varargs' and 'varkw' are the names of the * and ** arguments or None.
    'locals' is the locals dictionary of the given frame.
    """

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
    ) -> str:
        """Format an argument spec from the values returned by getfullargspec.

        The first seven arguments are (args, varargs, varkw, defaults,
        kwonlyargs, kwonlydefaults, annotations).  The other five arguments
        are the corresponding optional formatting functions that are called to
        turn names and values into strings.  The last argument is an optional
        function to format the sequence of arguments.

        Deprecated since Python 3.5: use the `signature` function and `Signature`
        objects.
        """

def formatargvalues(
    args: list[str],
    varargs: str | None,
    varkw: str | None,
    locals: Mapping[str, Any] | None,
    formatarg: Callable[[str], str] | None = ...,
    formatvarargs: Callable[[str], str] | None = ...,
    formatvarkw: Callable[[str], str] | None = ...,
    formatvalue: Callable[[Any], str] | None = ...,
) -> str:
    """Format an argument spec from the 4 values returned by getargvalues.

    The first four arguments are (args, varargs, varkw, locals).  The
    next four arguments are the corresponding optional formatting functions
    that are called to turn names and values into strings.  The ninth
    argument is an optional function to format the sequence of arguments.
    """

def getmro(cls: type) -> tuple[type, ...]:
    """Return tuple of base classes (including cls) in method resolution order."""

def getcallargs(func: Callable[_P, Any], /, *args: _P.args, **kwds: _P.kwargs) -> dict[str, Any]:
    """Get the mapping of arguments to values.

    A dict is returned, with keys the function argument names (including the
    names of the * and ** arguments, if any), and values the respective bound
    values from 'positional' and 'named'.
    """

class ClosureVars(NamedTuple):
    """ClosureVars(nonlocals, globals, builtins, unbound)"""

    nonlocals: Mapping[str, Any]
    globals: Mapping[str, Any]
    builtins: Mapping[str, Any]
    unbound: AbstractSet[str]

def getclosurevars(func: _IntrospectableCallable) -> ClosureVars:
    """
    Get the mapping of free variables to their current values.

    Returns a named tuple of dicts mapping the current nonlocal, global
    and builtin references as seen by the body of the function. A final
    set of unbound names that could not be resolved is also provided.
    """

def unwrap(func: Callable[..., Any], *, stop: Callable[[Callable[..., Any]], Any] | None = None) -> Any:
    """Get the object wrapped by *func*.

    Follows the chain of :attr:`__wrapped__` attributes returning the last
    object in the chain.

    *stop* is an optional callback accepting an object in the wrapper chain
    as its sole argument that allows the unwrapping to be terminated early if
    the callback returns a true value. If the callback never returns a true
    value, the last object in the chain is returned as usual. For example,
    :func:`signature` uses this to stop unwrapping if any object in the
    chain has a ``__signature__`` attribute defined.

    :exc:`ValueError` is raised if a cycle is encountered.

    """

#
# The interpreter stack
#

if sys.version_info >= (3, 11):
    class _Traceback(NamedTuple):
        """_Traceback(filename, lineno, function, code_context, index)"""

        filename: str
        lineno: int
        function: str
        code_context: list[str] | None
        index: int | None  # type: ignore[assignment]

    class _FrameInfo(NamedTuple):
        """_FrameInfo(frame, filename, lineno, function, code_context, index)"""

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
        """Traceback(filename, lineno, function, code_context, index)"""

        filename: str
        lineno: int
        function: str
        code_context: list[str] | None
        index: int | None  # type: ignore[assignment]

    class FrameInfo(NamedTuple):
        """FrameInfo(frame, filename, lineno, function, code_context, index)"""

        frame: FrameType
        filename: str
        lineno: int
        function: str
        code_context: list[str] | None
        index: int | None  # type: ignore[assignment]

def getframeinfo(frame: FrameType | TracebackType, context: int = 1) -> Traceback:
    """Get information about a frame or traceback object.

    A tuple of five things is returned: the filename, the line number of
    the current line, the function name, a list of lines of context from
    the source code, and the index of the current line within that list.
    The optional second argument specifies the number of lines of context
    to return, which are centered around the current line.
    """

def getouterframes(frame: Any, context: int = 1) -> list[FrameInfo]:
    """Get a list of records for a frame and all higher (calling) frames.

    Each record contains a frame object, filename, line number, function
    name, a list of lines of context, and index within the context.
    """

def getinnerframes(tb: TracebackType, context: int = 1) -> list[FrameInfo]:
    """Get a list of records for a traceback's frame and all lower frames.

    Each record contains a frame object, filename, line number, function
    name, a list of lines of context, and index within the context.
    """

def getlineno(frame: FrameType) -> int:
    """Get the line number from a frame object, allowing for optimization."""

def currentframe() -> FrameType | None:
    """Return the frame of the caller or None if this is not possible."""

def stack(context: int = 1) -> list[FrameInfo]:
    """Return a list of records for the stack above the caller's frame."""

def trace(context: int = 1) -> list[FrameInfo]:
    """Return a list of records for the stack below the current exception."""

#
# Fetching attributes statically
#

def getattr_static(obj: object, attr: str, default: Any | None = ...) -> Any:
    """Retrieve attributes without triggering dynamic lookup via the
    descriptor protocol,  __getattr__ or __getattribute__.

    Note: this function may not be able to retrieve all attributes
    that getattr can fetch (like dynamically created attributes)
    and may find attributes that getattr can't (like descriptors
    that raise AttributeError). It can also return descriptor objects
    instead of instance members in some cases. See the
    documentation for details.
    """

#
# Current State of Generators and Coroutines
#

GEN_CREATED: Final = "GEN_CREATED"
GEN_RUNNING: Final = "GEN_RUNNING"
GEN_SUSPENDED: Final = "GEN_SUSPENDED"
GEN_CLOSED: Final = "GEN_CLOSED"

def getgeneratorstate(
    generator: Generator[Any, Any, Any],
) -> Literal["GEN_CREATED", "GEN_RUNNING", "GEN_SUSPENDED", "GEN_CLOSED"]:
    """Get current state of a generator-iterator.

    Possible states are:
      GEN_CREATED: Waiting to start execution.
      GEN_RUNNING: Currently being executed by the interpreter.
      GEN_SUSPENDED: Currently suspended at a yield expression.
      GEN_CLOSED: Execution has completed.
    """

CORO_CREATED: Final = "CORO_CREATED"
CORO_RUNNING: Final = "CORO_RUNNING"
CORO_SUSPENDED: Final = "CORO_SUSPENDED"
CORO_CLOSED: Final = "CORO_CLOSED"

def getcoroutinestate(
    coroutine: Coroutine[Any, Any, Any],
) -> Literal["CORO_CREATED", "CORO_RUNNING", "CORO_SUSPENDED", "CORO_CLOSED"]:
    """Get current state of a coroutine object.

    Possible states are:
      CORO_CREATED: Waiting to start execution.
      CORO_RUNNING: Currently being executed by the interpreter.
      CORO_SUSPENDED: Currently suspended at an await expression.
      CORO_CLOSED: Execution has completed.
    """

def getgeneratorlocals(generator: Generator[Any, Any, Any]) -> dict[str, Any]:
    """
    Get the mapping of generator local variables to their current values.

    A dict is returned, with the keys the local variable names and values the
    bound values.
    """

def getcoroutinelocals(coroutine: Coroutine[Any, Any, Any]) -> dict[str, Any]:
    """
    Get the mapping of coroutine local variables to their current values.

    A dict is returned, with the keys the local variable names and values the
    bound values.
    """

# Create private type alias to avoid conflict with symbol of same
# name created in Attribute class.
_Object: TypeAlias = object

class Attribute(NamedTuple):
    """Attribute(name, kind, defining_class, object)"""

    name: str
    kind: Literal["class method", "static method", "property", "method", "data"]
    defining_class: type
    object: _Object

def classify_class_attrs(cls: type) -> list[Attribute]:
    """Return list of attribute-descriptor tuples.

    For each name in dir(cls), the return list contains a 4-tuple
    with these elements:

        0. The name (a string).

        1. The kind of attribute this is, one of these strings:
               'class method'    created via classmethod()
               'static method'   created via staticmethod()
               'property'        created via property()
               'method'          any other flavor of method or descriptor
               'data'            not a method

        2. The class which defined this attribute (a class).

        3. The object as obtained by calling getattr; if this fails, or if the
           resulting object does not live anywhere in the class' mro (including
           metaclasses) then the object is looked up in the defining class's
           dict (found by walking the mro).

    If one of the items in dir(cls) is stored in the metaclass it will now
    be discovered and not have None be listed as the class in which it was
    defined.  Any items whose home class cannot be discovered are skipped.
    """

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
