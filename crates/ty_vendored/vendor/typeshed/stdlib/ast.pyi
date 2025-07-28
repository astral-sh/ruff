import ast
import builtins
import os
import sys
import typing_extensions
from _ast import (
    PyCF_ALLOW_TOP_LEVEL_AWAIT as PyCF_ALLOW_TOP_LEVEL_AWAIT,
    PyCF_ONLY_AST as PyCF_ONLY_AST,
    PyCF_TYPE_COMMENTS as PyCF_TYPE_COMMENTS,
)
from _typeshed import ReadableBuffer, Unused
from collections.abc import Iterable, Iterator, Sequence
from typing import Any, ClassVar, Generic, Literal, TypedDict, TypeVar as _TypeVar, overload
from typing_extensions import Self, Unpack, deprecated

if sys.version_info >= (3, 13):
    from _ast import PyCF_OPTIMIZED_AST as PyCF_OPTIMIZED_AST

# Used for node end positions in constructor keyword arguments
_EndPositionT = typing_extensions.TypeVar("_EndPositionT", int, int | None, default=int | None)

# Corresponds to the names in the `_attributes` class variable which is non-empty in certain AST nodes
class _Attributes(TypedDict, Generic[_EndPositionT], total=False):
    lineno: int
    col_offset: int
    end_lineno: _EndPositionT
    end_col_offset: _EndPositionT

# The various AST classes are implemented in C, and imported from _ast at runtime,
# but they consider themselves to live in the ast module,
# so we'll define the stubs in this file.
class AST:
    if sys.version_info >= (3, 10):
        __match_args__ = ()
    _attributes: ClassVar[tuple[str, ...]]
    _fields: ClassVar[tuple[str, ...]]
    if sys.version_info >= (3, 13):
        _field_types: ClassVar[dict[str, Any]]

    if sys.version_info >= (3, 14):
        def __replace__(self) -> Self: ...

class mod(AST): ...

class Module(mod):
    if sys.version_info >= (3, 10):
        __match_args__ = ("body", "type_ignores")
    body: list[stmt]
    type_ignores: list[TypeIgnore]
    if sys.version_info >= (3, 13):
        def __init__(self, body: list[stmt] = ..., type_ignores: list[TypeIgnore] = ...) -> None: ...
    else:
        def __init__(self, body: list[stmt], type_ignores: list[TypeIgnore]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, body: list[stmt] = ..., type_ignores: list[TypeIgnore] = ...) -> Self: ...

class Interactive(mod):
    if sys.version_info >= (3, 10):
        __match_args__ = ("body",)
    body: list[stmt]
    if sys.version_info >= (3, 13):
        def __init__(self, body: list[stmt] = ...) -> None: ...
    else:
        def __init__(self, body: list[stmt]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, body: list[stmt] = ...) -> Self: ...

class Expression(mod):
    if sys.version_info >= (3, 10):
        __match_args__ = ("body",)
    body: expr
    def __init__(self, body: expr) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, body: expr = ...) -> Self: ...

class FunctionType(mod):
    if sys.version_info >= (3, 10):
        __match_args__ = ("argtypes", "returns")
    argtypes: list[expr]
    returns: expr
    if sys.version_info >= (3, 13):
        @overload
        def __init__(self, argtypes: list[expr], returns: expr) -> None: ...
        @overload
        def __init__(self, argtypes: list[expr] = ..., *, returns: expr) -> None: ...
    else:
        def __init__(self, argtypes: list[expr], returns: expr) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, argtypes: list[expr] = ..., returns: expr = ...) -> Self: ...

class stmt(AST):
    lineno: int
    col_offset: int
    end_lineno: int | None
    end_col_offset: int | None
    def __init__(self, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, **kwargs: Unpack[_Attributes]) -> Self: ...

class FunctionDef(stmt):
    if sys.version_info >= (3, 12):
        __match_args__ = ("name", "args", "body", "decorator_list", "returns", "type_comment", "type_params")
    elif sys.version_info >= (3, 10):
        __match_args__ = ("name", "args", "body", "decorator_list", "returns", "type_comment")
    name: str
    args: arguments
    body: list[stmt]
    decorator_list: list[expr]
    returns: expr | None
    type_comment: str | None
    if sys.version_info >= (3, 12):
        type_params: list[type_param]
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            name: str,
            args: arguments,
            body: list[stmt] = ...,
            decorator_list: list[expr] = ...,
            returns: expr | None = None,
            type_comment: str | None = None,
            type_params: list[type_param] = ...,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
    elif sys.version_info >= (3, 12):
        @overload
        def __init__(
            self,
            name: str,
            args: arguments,
            body: list[stmt],
            decorator_list: list[expr],
            returns: expr | None,
            type_comment: str | None,
            type_params: list[type_param],
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
        @overload
        def __init__(
            self,
            name: str,
            args: arguments,
            body: list[stmt],
            decorator_list: list[expr],
            returns: expr | None = None,
            type_comment: str | None = None,
            *,
            type_params: list[type_param],
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
    else:
        def __init__(
            self,
            name: str,
            args: arguments,
            body: list[stmt],
            decorator_list: list[expr],
            returns: expr | None = None,
            type_comment: str | None = None,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self,
            *,
            name: str = ...,
            args: arguments = ...,
            body: list[stmt] = ...,
            decorator_list: list[expr] = ...,
            returns: expr | None = ...,
            type_comment: str | None = ...,
            type_params: list[type_param] = ...,
            **kwargs: Unpack[_Attributes],
        ) -> Self: ...

class AsyncFunctionDef(stmt):
    if sys.version_info >= (3, 12):
        __match_args__ = ("name", "args", "body", "decorator_list", "returns", "type_comment", "type_params")
    elif sys.version_info >= (3, 10):
        __match_args__ = ("name", "args", "body", "decorator_list", "returns", "type_comment")
    name: str
    args: arguments
    body: list[stmt]
    decorator_list: list[expr]
    returns: expr | None
    type_comment: str | None
    if sys.version_info >= (3, 12):
        type_params: list[type_param]
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            name: str,
            args: arguments,
            body: list[stmt] = ...,
            decorator_list: list[expr] = ...,
            returns: expr | None = None,
            type_comment: str | None = None,
            type_params: list[type_param] = ...,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
    elif sys.version_info >= (3, 12):
        @overload
        def __init__(
            self,
            name: str,
            args: arguments,
            body: list[stmt],
            decorator_list: list[expr],
            returns: expr | None,
            type_comment: str | None,
            type_params: list[type_param],
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
        @overload
        def __init__(
            self,
            name: str,
            args: arguments,
            body: list[stmt],
            decorator_list: list[expr],
            returns: expr | None = None,
            type_comment: str | None = None,
            *,
            type_params: list[type_param],
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
    else:
        def __init__(
            self,
            name: str,
            args: arguments,
            body: list[stmt],
            decorator_list: list[expr],
            returns: expr | None = None,
            type_comment: str | None = None,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self,
            *,
            name: str = ...,
            args: arguments = ...,
            body: list[stmt] = ...,
            decorator_list: list[expr] = ...,
            returns: expr | None = ...,
            type_comment: str | None = ...,
            type_params: list[type_param] = ...,
            **kwargs: Unpack[_Attributes],
        ) -> Self: ...

class ClassDef(stmt):
    if sys.version_info >= (3, 12):
        __match_args__ = ("name", "bases", "keywords", "body", "decorator_list", "type_params")
    elif sys.version_info >= (3, 10):
        __match_args__ = ("name", "bases", "keywords", "body", "decorator_list")
    name: str
    bases: list[expr]
    keywords: list[keyword]
    body: list[stmt]
    decorator_list: list[expr]
    if sys.version_info >= (3, 12):
        type_params: list[type_param]
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            name: str,
            bases: list[expr] = ...,
            keywords: list[keyword] = ...,
            body: list[stmt] = ...,
            decorator_list: list[expr] = ...,
            type_params: list[type_param] = ...,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
    elif sys.version_info >= (3, 12):
        def __init__(
            self,
            name: str,
            bases: list[expr],
            keywords: list[keyword],
            body: list[stmt],
            decorator_list: list[expr],
            type_params: list[type_param],
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
    else:
        def __init__(
            self,
            name: str,
            bases: list[expr],
            keywords: list[keyword],
            body: list[stmt],
            decorator_list: list[expr],
            **kwargs: Unpack[_Attributes],
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self,
            *,
            name: str = ...,
            bases: list[expr] = ...,
            keywords: list[keyword] = ...,
            body: list[stmt] = ...,
            decorator_list: list[expr] = ...,
            type_params: list[type_param] = ...,
            **kwargs: Unpack[_Attributes],
        ) -> Self: ...

class Return(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value",)
    value: expr | None
    def __init__(self, value: expr | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, value: expr | None = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Delete(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("targets",)
    targets: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, targets: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, targets: list[expr], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, targets: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Assign(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("targets", "value", "type_comment")
    targets: list[expr]
    value: expr
    type_comment: str | None
    if sys.version_info >= (3, 13):
        @overload
        def __init__(
            self, targets: list[expr], value: expr, type_comment: str | None = None, **kwargs: Unpack[_Attributes]
        ) -> None: ...
        @overload
        def __init__(
            self, targets: list[expr] = ..., *, value: expr, type_comment: str | None = None, **kwargs: Unpack[_Attributes]
        ) -> None: ...
    else:
        def __init__(
            self, targets: list[expr], value: expr, type_comment: str | None = None, **kwargs: Unpack[_Attributes]
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, targets: list[expr] = ..., value: expr = ..., type_comment: str | None = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

if sys.version_info >= (3, 12):
    class TypeAlias(stmt):
        __match_args__ = ("name", "type_params", "value")
        name: Name
        type_params: list[type_param]
        value: expr
        if sys.version_info >= (3, 13):
            @overload
            def __init__(
                self, name: Name, type_params: list[type_param], value: expr, **kwargs: Unpack[_Attributes[int]]
            ) -> None: ...
            @overload
            def __init__(
                self, name: Name, type_params: list[type_param] = ..., *, value: expr, **kwargs: Unpack[_Attributes[int]]
            ) -> None: ...
        else:
            def __init__(
                self, name: Name, type_params: list[type_param], value: expr, **kwargs: Unpack[_Attributes[int]]
            ) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(  # type: ignore[override]
                self,
                *,
                name: Name = ...,
                type_params: list[type_param] = ...,
                value: expr = ...,
                **kwargs: Unpack[_Attributes[int]],
            ) -> Self: ...

class AugAssign(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("target", "op", "value")
    target: Name | Attribute | Subscript
    op: operator
    value: expr
    def __init__(
        self, target: Name | Attribute | Subscript, op: operator, value: expr, **kwargs: Unpack[_Attributes]
    ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self,
            *,
            target: Name | Attribute | Subscript = ...,
            op: operator = ...,
            value: expr = ...,
            **kwargs: Unpack[_Attributes],
        ) -> Self: ...

class AnnAssign(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("target", "annotation", "value", "simple")
    target: Name | Attribute | Subscript
    annotation: expr
    value: expr | None
    simple: int
    @overload
    def __init__(
        self,
        target: Name | Attribute | Subscript,
        annotation: expr,
        value: expr | None,
        simple: int,
        **kwargs: Unpack[_Attributes],
    ) -> None: ...
    @overload
    def __init__(
        self,
        target: Name | Attribute | Subscript,
        annotation: expr,
        value: expr | None = None,
        *,
        simple: int,
        **kwargs: Unpack[_Attributes],
    ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self,
            *,
            target: Name | Attribute | Subscript = ...,
            annotation: expr = ...,
            value: expr | None = ...,
            simple: int = ...,
            **kwargs: Unpack[_Attributes],
        ) -> Self: ...

class For(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("target", "iter", "body", "orelse", "type_comment")
    target: expr
    iter: expr
    body: list[stmt]
    orelse: list[stmt]
    type_comment: str | None
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            target: expr,
            iter: expr,
            body: list[stmt] = ...,
            orelse: list[stmt] = ...,
            type_comment: str | None = None,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
    else:
        def __init__(
            self,
            target: expr,
            iter: expr,
            body: list[stmt],
            orelse: list[stmt],
            type_comment: str | None = None,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self,
            *,
            target: expr = ...,
            iter: expr = ...,
            body: list[stmt] = ...,
            orelse: list[stmt] = ...,
            type_comment: str | None = ...,
            **kwargs: Unpack[_Attributes],
        ) -> Self: ...

class AsyncFor(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("target", "iter", "body", "orelse", "type_comment")
    target: expr
    iter: expr
    body: list[stmt]
    orelse: list[stmt]
    type_comment: str | None
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            target: expr,
            iter: expr,
            body: list[stmt] = ...,
            orelse: list[stmt] = ...,
            type_comment: str | None = None,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
    else:
        def __init__(
            self,
            target: expr,
            iter: expr,
            body: list[stmt],
            orelse: list[stmt],
            type_comment: str | None = None,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self,
            *,
            target: expr = ...,
            iter: expr = ...,
            body: list[stmt] = ...,
            orelse: list[stmt] = ...,
            type_comment: str | None = ...,
            **kwargs: Unpack[_Attributes],
        ) -> Self: ...

class While(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("test", "body", "orelse")
    test: expr
    body: list[stmt]
    orelse: list[stmt]
    if sys.version_info >= (3, 13):
        def __init__(
            self, test: expr, body: list[stmt] = ..., orelse: list[stmt] = ..., **kwargs: Unpack[_Attributes]
        ) -> None: ...
    else:
        def __init__(self, test: expr, body: list[stmt], orelse: list[stmt], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, test: expr = ..., body: list[stmt] = ..., orelse: list[stmt] = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class If(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("test", "body", "orelse")
    test: expr
    body: list[stmt]
    orelse: list[stmt]
    if sys.version_info >= (3, 13):
        def __init__(
            self, test: expr, body: list[stmt] = ..., orelse: list[stmt] = ..., **kwargs: Unpack[_Attributes]
        ) -> None: ...
    else:
        def __init__(self, test: expr, body: list[stmt], orelse: list[stmt], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, test: expr = ..., body: list[stmt] = ..., orelse: list[stmt] = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class With(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("items", "body", "type_comment")
    items: list[withitem]
    body: list[stmt]
    type_comment: str | None
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            items: list[withitem] = ...,
            body: list[stmt] = ...,
            type_comment: str | None = None,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
    else:
        def __init__(
            self, items: list[withitem], body: list[stmt], type_comment: str | None = None, **kwargs: Unpack[_Attributes]
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self,
            *,
            items: list[withitem] = ...,
            body: list[stmt] = ...,
            type_comment: str | None = ...,
            **kwargs: Unpack[_Attributes],
        ) -> Self: ...

class AsyncWith(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("items", "body", "type_comment")
    items: list[withitem]
    body: list[stmt]
    type_comment: str | None
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            items: list[withitem] = ...,
            body: list[stmt] = ...,
            type_comment: str | None = None,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
    else:
        def __init__(
            self, items: list[withitem], body: list[stmt], type_comment: str | None = None, **kwargs: Unpack[_Attributes]
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self,
            *,
            items: list[withitem] = ...,
            body: list[stmt] = ...,
            type_comment: str | None = ...,
            **kwargs: Unpack[_Attributes],
        ) -> Self: ...

class Raise(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("exc", "cause")
    exc: expr | None
    cause: expr | None
    def __init__(self, exc: expr | None = None, cause: expr | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, exc: expr | None = ..., cause: expr | None = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Try(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("body", "handlers", "orelse", "finalbody")
    body: list[stmt]
    handlers: list[ExceptHandler]
    orelse: list[stmt]
    finalbody: list[stmt]
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            body: list[stmt] = ...,
            handlers: list[ExceptHandler] = ...,
            orelse: list[stmt] = ...,
            finalbody: list[stmt] = ...,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
    else:
        def __init__(
            self,
            body: list[stmt],
            handlers: list[ExceptHandler],
            orelse: list[stmt],
            finalbody: list[stmt],
            **kwargs: Unpack[_Attributes],
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self,
            *,
            body: list[stmt] = ...,
            handlers: list[ExceptHandler] = ...,
            orelse: list[stmt] = ...,
            finalbody: list[stmt] = ...,
            **kwargs: Unpack[_Attributes],
        ) -> Self: ...

if sys.version_info >= (3, 11):
    class TryStar(stmt):
        __match_args__ = ("body", "handlers", "orelse", "finalbody")
        body: list[stmt]
        handlers: list[ExceptHandler]
        orelse: list[stmt]
        finalbody: list[stmt]
        if sys.version_info >= (3, 13):
            def __init__(
                self,
                body: list[stmt] = ...,
                handlers: list[ExceptHandler] = ...,
                orelse: list[stmt] = ...,
                finalbody: list[stmt] = ...,
                **kwargs: Unpack[_Attributes],
            ) -> None: ...
        else:
            def __init__(
                self,
                body: list[stmt],
                handlers: list[ExceptHandler],
                orelse: list[stmt],
                finalbody: list[stmt],
                **kwargs: Unpack[_Attributes],
            ) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(
                self,
                *,
                body: list[stmt] = ...,
                handlers: list[ExceptHandler] = ...,
                orelse: list[stmt] = ...,
                finalbody: list[stmt] = ...,
                **kwargs: Unpack[_Attributes],
            ) -> Self: ...

class Assert(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("test", "msg")
    test: expr
    msg: expr | None
    def __init__(self, test: expr, msg: expr | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, test: expr = ..., msg: expr | None = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Import(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("names",)
    names: list[alias]
    if sys.version_info >= (3, 13):
        def __init__(self, names: list[alias] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, names: list[alias], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, names: list[alias] = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class ImportFrom(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("module", "names", "level")
    module: str | None
    names: list[alias]
    level: int
    if sys.version_info >= (3, 13):
        @overload
        def __init__(self, module: str | None, names: list[alias], level: int, **kwargs: Unpack[_Attributes]) -> None: ...
        @overload
        def __init__(
            self, module: str | None = None, names: list[alias] = ..., *, level: int, **kwargs: Unpack[_Attributes]
        ) -> None: ...
    else:
        @overload
        def __init__(self, module: str | None, names: list[alias], level: int, **kwargs: Unpack[_Attributes]) -> None: ...
        @overload
        def __init__(
            self, module: str | None = None, *, names: list[alias], level: int, **kwargs: Unpack[_Attributes]
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, module: str | None = ..., names: list[alias] = ..., level: int = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class Global(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("names",)
    names: list[str]
    if sys.version_info >= (3, 13):
        def __init__(self, names: list[str] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, names: list[str], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, names: list[str] = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Nonlocal(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("names",)
    names: list[str]
    if sys.version_info >= (3, 13):
        def __init__(self, names: list[str] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, names: list[str], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, names: list[str] = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Expr(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value",)
    value: expr
    def __init__(self, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, value: expr = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Pass(stmt): ...
class Break(stmt): ...
class Continue(stmt): ...

class expr(AST):
    lineno: int
    col_offset: int
    end_lineno: int | None
    end_col_offset: int | None
    def __init__(self, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, **kwargs: Unpack[_Attributes]) -> Self: ...

class BoolOp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("op", "values")
    op: boolop
    values: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, op: boolop, values: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, op: boolop, values: list[expr], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, op: boolop = ..., values: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class NamedExpr(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("target", "value")
    target: Name
    value: expr
    def __init__(self, target: Name, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, target: Name = ..., value: expr = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class BinOp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("left", "op", "right")
    left: expr
    op: operator
    right: expr
    def __init__(self, left: expr, op: operator, right: expr, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, left: expr = ..., op: operator = ..., right: expr = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class UnaryOp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("op", "operand")
    op: unaryop
    operand: expr
    def __init__(self, op: unaryop, operand: expr, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, op: unaryop = ..., operand: expr = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Lambda(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("args", "body")
    args: arguments
    body: expr
    def __init__(self, args: arguments, body: expr, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, args: arguments = ..., body: expr = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class IfExp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("test", "body", "orelse")
    test: expr
    body: expr
    orelse: expr
    def __init__(self, test: expr, body: expr, orelse: expr, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, test: expr = ..., body: expr = ..., orelse: expr = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class Dict(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("keys", "values")
    keys: list[expr | None]
    values: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, keys: list[expr | None] = ..., values: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, keys: list[expr | None], values: list[expr], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, keys: list[expr | None] = ..., values: list[expr] = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class Set(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elts",)
    elts: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, elts: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elts: list[expr], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, elts: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class ListComp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elt", "generators")
    elt: expr
    generators: list[comprehension]
    if sys.version_info >= (3, 13):
        def __init__(self, elt: expr, generators: list[comprehension] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elt: expr, generators: list[comprehension], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, elt: expr = ..., generators: list[comprehension] = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class SetComp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elt", "generators")
    elt: expr
    generators: list[comprehension]
    if sys.version_info >= (3, 13):
        def __init__(self, elt: expr, generators: list[comprehension] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elt: expr, generators: list[comprehension], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, elt: expr = ..., generators: list[comprehension] = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class DictComp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("key", "value", "generators")
    key: expr
    value: expr
    generators: list[comprehension]
    if sys.version_info >= (3, 13):
        def __init__(
            self, key: expr, value: expr, generators: list[comprehension] = ..., **kwargs: Unpack[_Attributes]
        ) -> None: ...
    else:
        def __init__(self, key: expr, value: expr, generators: list[comprehension], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, key: expr = ..., value: expr = ..., generators: list[comprehension] = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class GeneratorExp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elt", "generators")
    elt: expr
    generators: list[comprehension]
    if sys.version_info >= (3, 13):
        def __init__(self, elt: expr, generators: list[comprehension] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elt: expr, generators: list[comprehension], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, elt: expr = ..., generators: list[comprehension] = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class Await(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value",)
    value: expr
    def __init__(self, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, value: expr = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Yield(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value",)
    value: expr | None
    def __init__(self, value: expr | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, value: expr | None = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class YieldFrom(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value",)
    value: expr
    def __init__(self, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, value: expr = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Compare(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("left", "ops", "comparators")
    left: expr
    ops: list[cmpop]
    comparators: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(
            self, left: expr, ops: list[cmpop] = ..., comparators: list[expr] = ..., **kwargs: Unpack[_Attributes]
        ) -> None: ...
    else:
        def __init__(self, left: expr, ops: list[cmpop], comparators: list[expr], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, left: expr = ..., ops: list[cmpop] = ..., comparators: list[expr] = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class Call(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("func", "args", "keywords")
    func: expr
    args: list[expr]
    keywords: list[keyword]
    if sys.version_info >= (3, 13):
        def __init__(
            self, func: expr, args: list[expr] = ..., keywords: list[keyword] = ..., **kwargs: Unpack[_Attributes]
        ) -> None: ...
    else:
        def __init__(self, func: expr, args: list[expr], keywords: list[keyword], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, func: expr = ..., args: list[expr] = ..., keywords: list[keyword] = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class FormattedValue(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value", "conversion", "format_spec")
    value: expr
    conversion: int
    format_spec: expr | None
    def __init__(self, value: expr, conversion: int, format_spec: expr | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, value: expr = ..., conversion: int = ..., format_spec: expr | None = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class JoinedStr(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("values",)
    values: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, values: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, values: list[expr], **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, values: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

if sys.version_info >= (3, 14):
    class TemplateStr(expr):
        __match_args__ = ("values",)
        values: list[expr]
        def __init__(self, values: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
        def __replace__(self, *, values: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

    class Interpolation(expr):
        __match_args__ = ("value", "str", "conversion", "format_spec")
        value: expr
        str: builtins.str
        conversion: int
        format_spec: expr | None = None
        def __init__(
            self,
            value: expr = ...,
            str: builtins.str = ...,
            conversion: int = ...,
            format_spec: expr | None = ...,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...
        def __replace__(
            self,
            *,
            value: expr = ...,
            str: builtins.str = ...,
            conversion: int = ...,
            format_spec: expr | None = ...,
            **kwargs: Unpack[_Attributes],
        ) -> Self: ...

if sys.version_info >= (3, 10):
    from types import EllipsisType

    _ConstantValue: typing_extensions.TypeAlias = str | bytes | bool | int | float | complex | None | EllipsisType
else:
    # Rely on builtins.ellipsis
    _ConstantValue: typing_extensions.TypeAlias = str | bytes | bool | int | float | complex | None | ellipsis  # noqa: F821

class Constant(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value", "kind")
    value: _ConstantValue
    kind: str | None
    if sys.version_info < (3, 14):
        # Aliases for value, for backwards compatibility
        @deprecated("Will be removed in Python 3.14; use value instead")
        @property
        def n(self) -> _ConstantValue: ...
        @n.setter
        def n(self, value: _ConstantValue) -> None: ...
        @deprecated("Will be removed in Python 3.14; use value instead")
        @property
        def s(self) -> _ConstantValue: ...
        @s.setter
        def s(self, value: _ConstantValue) -> None: ...

    def __init__(self, value: _ConstantValue, kind: str | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, value: _ConstantValue = ..., kind: str | None = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Attribute(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value", "attr", "ctx")
    value: expr
    attr: str
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    def __init__(self, value: expr, attr: str, ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, value: expr = ..., attr: str = ..., ctx: expr_context = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class Subscript(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value", "slice", "ctx")
    value: expr
    slice: expr
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    def __init__(self, value: expr, slice: expr, ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, value: expr = ..., slice: expr = ..., ctx: expr_context = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class Starred(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value", "ctx")
    value: expr
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    def __init__(self, value: expr, ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, value: expr = ..., ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Name(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("id", "ctx")
    id: str
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    def __init__(self, id: str, ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, id: str = ..., ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class List(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elts", "ctx")
    elts: list[expr]
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    if sys.version_info >= (3, 13):
        def __init__(self, elts: list[expr] = ..., ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elts: list[expr], ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, elts: list[expr] = ..., ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class Tuple(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elts", "ctx")
    elts: list[expr]
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    dims: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, elts: list[expr] = ..., ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elts: list[expr], ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, elts: list[expr] = ..., ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

@deprecated("Deprecated since Python 3.9.")
class slice(AST): ...

class Slice(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("lower", "upper", "step")
    lower: expr | None
    upper: expr | None
    step: expr | None
    def __init__(
        self, lower: expr | None = None, upper: expr | None = None, step: expr | None = None, **kwargs: Unpack[_Attributes]
    ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, lower: expr | None = ..., upper: expr | None = ..., step: expr | None = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

@deprecated("Deprecated since Python 3.9. Use ast.Tuple instead.")
class ExtSlice(slice):
    def __new__(cls, dims: Iterable[slice] = (), **kwargs: Unpack[_Attributes]) -> Tuple: ...  # type: ignore[misc]

@deprecated("Deprecated since Python 3.9. Use the index value directly instead.")
class Index(slice):
    def __new__(cls, value: expr, **kwargs: Unpack[_Attributes]) -> expr: ...  # type: ignore[misc]

class expr_context(AST): ...

@deprecated("Deprecated since Python 3.9. Unused in Python 3.")
class AugLoad(expr_context): ...

@deprecated("Deprecated since Python 3.9. Unused in Python 3.")
class AugStore(expr_context): ...

@deprecated("Deprecated since Python 3.9. Unused in Python 3.")
class Param(expr_context): ...

@deprecated("Deprecated since Python 3.9. Unused in Python 3.")
class Suite(mod): ...

class Load(expr_context): ...
class Store(expr_context): ...
class Del(expr_context): ...
class boolop(AST): ...
class And(boolop): ...
class Or(boolop): ...
class operator(AST): ...
class Add(operator): ...
class Sub(operator): ...
class Mult(operator): ...
class MatMult(operator): ...
class Div(operator): ...
class Mod(operator): ...
class Pow(operator): ...
class LShift(operator): ...
class RShift(operator): ...
class BitOr(operator): ...
class BitXor(operator): ...
class BitAnd(operator): ...
class FloorDiv(operator): ...
class unaryop(AST): ...
class Invert(unaryop): ...
class Not(unaryop): ...
class UAdd(unaryop): ...
class USub(unaryop): ...
class cmpop(AST): ...
class Eq(cmpop): ...
class NotEq(cmpop): ...
class Lt(cmpop): ...
class LtE(cmpop): ...
class Gt(cmpop): ...
class GtE(cmpop): ...
class Is(cmpop): ...
class IsNot(cmpop): ...
class In(cmpop): ...
class NotIn(cmpop): ...

class comprehension(AST):
    if sys.version_info >= (3, 10):
        __match_args__ = ("target", "iter", "ifs", "is_async")
    target: expr
    iter: expr
    ifs: list[expr]
    is_async: int
    if sys.version_info >= (3, 13):
        @overload
        def __init__(self, target: expr, iter: expr, ifs: list[expr], is_async: int) -> None: ...
        @overload
        def __init__(self, target: expr, iter: expr, ifs: list[expr] = ..., *, is_async: int) -> None: ...
    else:
        def __init__(self, target: expr, iter: expr, ifs: list[expr], is_async: int) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, target: expr = ..., iter: expr = ..., ifs: list[expr] = ..., is_async: int = ...) -> Self: ...

class excepthandler(AST):
    lineno: int
    col_offset: int
    end_lineno: int | None
    end_col_offset: int | None
    def __init__(self, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, lineno: int = ..., col_offset: int = ..., end_lineno: int | None = ..., end_col_offset: int | None = ...
        ) -> Self: ...

class ExceptHandler(excepthandler):
    if sys.version_info >= (3, 10):
        __match_args__ = ("type", "name", "body")
    type: expr | None
    name: str | None
    body: list[stmt]
    if sys.version_info >= (3, 13):
        def __init__(
            self, type: expr | None = None, name: str | None = None, body: list[stmt] = ..., **kwargs: Unpack[_Attributes]
        ) -> None: ...
    else:
        @overload
        def __init__(self, type: expr | None, name: str | None, body: list[stmt], **kwargs: Unpack[_Attributes]) -> None: ...
        @overload
        def __init__(
            self, type: expr | None = None, name: str | None = None, *, body: list[stmt], **kwargs: Unpack[_Attributes]
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, type: expr | None = ..., name: str | None = ..., body: list[stmt] = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class arguments(AST):
    if sys.version_info >= (3, 10):
        __match_args__ = ("posonlyargs", "args", "vararg", "kwonlyargs", "kw_defaults", "kwarg", "defaults")
    posonlyargs: list[arg]
    args: list[arg]
    vararg: arg | None
    kwonlyargs: list[arg]
    kw_defaults: list[expr | None]
    kwarg: arg | None
    defaults: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            posonlyargs: list[arg] = ...,
            args: list[arg] = ...,
            vararg: arg | None = None,
            kwonlyargs: list[arg] = ...,
            kw_defaults: list[expr | None] = ...,
            kwarg: arg | None = None,
            defaults: list[expr] = ...,
        ) -> None: ...
    else:
        @overload
        def __init__(
            self,
            posonlyargs: list[arg],
            args: list[arg],
            vararg: arg | None,
            kwonlyargs: list[arg],
            kw_defaults: list[expr | None],
            kwarg: arg | None,
            defaults: list[expr],
        ) -> None: ...
        @overload
        def __init__(
            self,
            posonlyargs: list[arg],
            args: list[arg],
            vararg: arg | None,
            kwonlyargs: list[arg],
            kw_defaults: list[expr | None],
            kwarg: arg | None = None,
            *,
            defaults: list[expr],
        ) -> None: ...
        @overload
        def __init__(
            self,
            posonlyargs: list[arg],
            args: list[arg],
            vararg: arg | None = None,
            *,
            kwonlyargs: list[arg],
            kw_defaults: list[expr | None],
            kwarg: arg | None = None,
            defaults: list[expr],
        ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self,
            *,
            posonlyargs: list[arg] = ...,
            args: list[arg] = ...,
            vararg: arg | None = ...,
            kwonlyargs: list[arg] = ...,
            kw_defaults: list[expr | None] = ...,
            kwarg: arg | None = ...,
            defaults: list[expr] = ...,
        ) -> Self: ...

class arg(AST):
    lineno: int
    col_offset: int
    end_lineno: int | None
    end_col_offset: int | None
    if sys.version_info >= (3, 10):
        __match_args__ = ("arg", "annotation", "type_comment")
    arg: str
    annotation: expr | None
    type_comment: str | None
    def __init__(
        self, arg: str, annotation: expr | None = None, type_comment: str | None = None, **kwargs: Unpack[_Attributes]
    ) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(
            self, *, arg: str = ..., annotation: expr | None = ..., type_comment: str | None = ..., **kwargs: Unpack[_Attributes]
        ) -> Self: ...

class keyword(AST):
    lineno: int
    col_offset: int
    end_lineno: int | None
    end_col_offset: int | None
    if sys.version_info >= (3, 10):
        __match_args__ = ("arg", "value")
    arg: str | None
    value: expr
    @overload
    def __init__(self, arg: str | None, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...
    @overload
    def __init__(self, arg: str | None = None, *, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, arg: str | None = ..., value: expr = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class alias(AST):
    name: str
    asname: str | None
    if sys.version_info >= (3, 10):
        lineno: int
        col_offset: int
        end_lineno: int | None
        end_col_offset: int | None
    if sys.version_info >= (3, 10):
        __match_args__ = ("name", "asname")
    if sys.version_info >= (3, 10):
        def __init__(self, name: str, asname: str | None = None, **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, name: str, asname: str | None = None) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, name: str = ..., asname: str | None = ..., **kwargs: Unpack[_Attributes]) -> Self: ...

class withitem(AST):
    if sys.version_info >= (3, 10):
        __match_args__ = ("context_expr", "optional_vars")
    context_expr: expr
    optional_vars: expr | None
    def __init__(self, context_expr: expr, optional_vars: expr | None = None) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, context_expr: expr = ..., optional_vars: expr | None = ...) -> Self: ...

if sys.version_info >= (3, 10):
    class pattern(AST):
        lineno: int
        col_offset: int
        end_lineno: int
        end_col_offset: int
        def __init__(self, **kwargs: Unpack[_Attributes[int]]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(
                self, *, lineno: int = ..., col_offset: int = ..., end_lineno: int = ..., end_col_offset: int = ...
            ) -> Self: ...

    class match_case(AST):
        __match_args__ = ("pattern", "guard", "body")
        pattern: ast.pattern
        guard: expr | None
        body: list[stmt]
        if sys.version_info >= (3, 13):
            def __init__(self, pattern: ast.pattern, guard: expr | None = None, body: list[stmt] = ...) -> None: ...
        elif sys.version_info >= (3, 10):
            @overload
            def __init__(self, pattern: ast.pattern, guard: expr | None, body: list[stmt]) -> None: ...
            @overload
            def __init__(self, pattern: ast.pattern, guard: expr | None = None, *, body: list[stmt]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(self, *, pattern: ast.pattern = ..., guard: expr | None = ..., body: list[stmt] = ...) -> Self: ...

    class Match(stmt):
        __match_args__ = ("subject", "cases")
        subject: expr
        cases: list[match_case]
        if sys.version_info >= (3, 13):
            def __init__(self, subject: expr, cases: list[match_case] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
        else:
            def __init__(self, subject: expr, cases: list[match_case], **kwargs: Unpack[_Attributes]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(
                self, *, subject: expr = ..., cases: list[match_case] = ..., **kwargs: Unpack[_Attributes]
            ) -> Self: ...

    class MatchValue(pattern):
        __match_args__ = ("value",)
        value: expr
        def __init__(self, value: expr, **kwargs: Unpack[_Attributes[int]]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(self, *, value: expr = ..., **kwargs: Unpack[_Attributes[int]]) -> Self: ...

    class MatchSingleton(pattern):
        __match_args__ = ("value",)
        value: bool | None
        def __init__(self, value: bool | None, **kwargs: Unpack[_Attributes[int]]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(self, *, value: bool | None = ..., **kwargs: Unpack[_Attributes[int]]) -> Self: ...

    class MatchSequence(pattern):
        __match_args__ = ("patterns",)
        patterns: list[pattern]
        if sys.version_info >= (3, 13):
            def __init__(self, patterns: list[pattern] = ..., **kwargs: Unpack[_Attributes[int]]) -> None: ...
        else:
            def __init__(self, patterns: list[pattern], **kwargs: Unpack[_Attributes[int]]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(self, *, patterns: list[pattern] = ..., **kwargs: Unpack[_Attributes[int]]) -> Self: ...

    class MatchMapping(pattern):
        __match_args__ = ("keys", "patterns", "rest")
        keys: list[expr]
        patterns: list[pattern]
        rest: str | None
        if sys.version_info >= (3, 13):
            def __init__(
                self,
                keys: list[expr] = ...,
                patterns: list[pattern] = ...,
                rest: str | None = None,
                **kwargs: Unpack[_Attributes[int]],
            ) -> None: ...
        else:
            def __init__(
                self, keys: list[expr], patterns: list[pattern], rest: str | None = None, **kwargs: Unpack[_Attributes[int]]
            ) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(
                self,
                *,
                keys: list[expr] = ...,
                patterns: list[pattern] = ...,
                rest: str | None = ...,
                **kwargs: Unpack[_Attributes[int]],
            ) -> Self: ...

    class MatchClass(pattern):
        __match_args__ = ("cls", "patterns", "kwd_attrs", "kwd_patterns")
        cls: expr
        patterns: list[pattern]
        kwd_attrs: list[str]
        kwd_patterns: list[pattern]
        if sys.version_info >= (3, 13):
            def __init__(
                self,
                cls: expr,
                patterns: list[pattern] = ...,
                kwd_attrs: list[str] = ...,
                kwd_patterns: list[pattern] = ...,
                **kwargs: Unpack[_Attributes[int]],
            ) -> None: ...
        else:
            def __init__(
                self,
                cls: expr,
                patterns: list[pattern],
                kwd_attrs: list[str],
                kwd_patterns: list[pattern],
                **kwargs: Unpack[_Attributes[int]],
            ) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(
                self,
                *,
                cls: expr = ...,
                patterns: list[pattern] = ...,
                kwd_attrs: list[str] = ...,
                kwd_patterns: list[pattern] = ...,
                **kwargs: Unpack[_Attributes[int]],
            ) -> Self: ...

    class MatchStar(pattern):
        __match_args__ = ("name",)
        name: str | None
        def __init__(self, name: str | None = None, **kwargs: Unpack[_Attributes[int]]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(self, *, name: str | None = ..., **kwargs: Unpack[_Attributes[int]]) -> Self: ...

    class MatchAs(pattern):
        __match_args__ = ("pattern", "name")
        pattern: ast.pattern | None
        name: str | None
        def __init__(
            self, pattern: ast.pattern | None = None, name: str | None = None, **kwargs: Unpack[_Attributes[int]]
        ) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(
                self, *, pattern: ast.pattern | None = ..., name: str | None = ..., **kwargs: Unpack[_Attributes[int]]
            ) -> Self: ...

    class MatchOr(pattern):
        __match_args__ = ("patterns",)
        patterns: list[pattern]
        if sys.version_info >= (3, 13):
            def __init__(self, patterns: list[pattern] = ..., **kwargs: Unpack[_Attributes[int]]) -> None: ...
        else:
            def __init__(self, patterns: list[pattern], **kwargs: Unpack[_Attributes[int]]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(self, *, patterns: list[pattern] = ..., **kwargs: Unpack[_Attributes[int]]) -> Self: ...

class type_ignore(AST): ...

class TypeIgnore(type_ignore):
    if sys.version_info >= (3, 10):
        __match_args__ = ("lineno", "tag")
    lineno: int
    tag: str
    def __init__(self, lineno: int, tag: str) -> None: ...

    if sys.version_info >= (3, 14):
        def __replace__(self, *, lineno: int = ..., tag: str = ...) -> Self: ...

if sys.version_info >= (3, 12):
    class type_param(AST):
        lineno: int
        col_offset: int
        end_lineno: int
        end_col_offset: int
        def __init__(self, **kwargs: Unpack[_Attributes[int]]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(self, **kwargs: Unpack[_Attributes[int]]) -> Self: ...

    class TypeVar(type_param):
        if sys.version_info >= (3, 13):
            __match_args__ = ("name", "bound", "default_value")
        else:
            __match_args__ = ("name", "bound")
        name: str
        bound: expr | None
        if sys.version_info >= (3, 13):
            default_value: expr | None
            def __init__(
                self, name: str, bound: expr | None = None, default_value: expr | None = None, **kwargs: Unpack[_Attributes[int]]
            ) -> None: ...
        else:
            def __init__(self, name: str, bound: expr | None = None, **kwargs: Unpack[_Attributes[int]]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(
                self,
                *,
                name: str = ...,
                bound: expr | None = ...,
                default_value: expr | None = ...,
                **kwargs: Unpack[_Attributes[int]],
            ) -> Self: ...

    class ParamSpec(type_param):
        if sys.version_info >= (3, 13):
            __match_args__ = ("name", "default_value")
        else:
            __match_args__ = ("name",)
        name: str
        if sys.version_info >= (3, 13):
            default_value: expr | None
            def __init__(self, name: str, default_value: expr | None = None, **kwargs: Unpack[_Attributes[int]]) -> None: ...
        else:
            def __init__(self, name: str, **kwargs: Unpack[_Attributes[int]]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(
                self, *, name: str = ..., default_value: expr | None = ..., **kwargs: Unpack[_Attributes[int]]
            ) -> Self: ...

    class TypeVarTuple(type_param):
        if sys.version_info >= (3, 13):
            __match_args__ = ("name", "default_value")
        else:
            __match_args__ = ("name",)
        name: str
        if sys.version_info >= (3, 13):
            default_value: expr | None
            def __init__(self, name: str, default_value: expr | None = None, **kwargs: Unpack[_Attributes[int]]) -> None: ...
        else:
            def __init__(self, name: str, **kwargs: Unpack[_Attributes[int]]) -> None: ...

        if sys.version_info >= (3, 14):
            def __replace__(
                self, *, name: str = ..., default_value: expr | None = ..., **kwargs: Unpack[_Attributes[int]]
            ) -> Self: ...

class _ABC(type):
    def __init__(cls, *args: Unused) -> None: ...

if sys.version_info < (3, 14):
    @deprecated("Replaced by ast.Constant; removed in Python 3.14")
    class Num(Constant, metaclass=_ABC):
        def __new__(cls, n: complex, **kwargs: Unpack[_Attributes]) -> Constant: ...  # type: ignore[misc]  # pyright: ignore[reportInconsistentConstructor]

    @deprecated("Replaced by ast.Constant; removed in Python 3.14")
    class Str(Constant, metaclass=_ABC):
        def __new__(cls, s: str, **kwargs: Unpack[_Attributes]) -> Constant: ...  # type: ignore[misc]  # pyright: ignore[reportInconsistentConstructor]

    @deprecated("Replaced by ast.Constant; removed in Python 3.14")
    class Bytes(Constant, metaclass=_ABC):
        def __new__(cls, s: bytes, **kwargs: Unpack[_Attributes]) -> Constant: ...  # type: ignore[misc]  # pyright: ignore[reportInconsistentConstructor]

    @deprecated("Replaced by ast.Constant; removed in Python 3.14")
    class NameConstant(Constant, metaclass=_ABC):
        def __new__(cls, value: _ConstantValue, kind: str | None, **kwargs: Unpack[_Attributes]) -> Constant: ...  # type: ignore[misc]  # pyright: ignore[reportInconsistentConstructor]

    @deprecated("Replaced by ast.Constant; removed in Python 3.14")
    class Ellipsis(Constant, metaclass=_ABC):
        def __new__(cls, **kwargs: Unpack[_Attributes]) -> Constant: ...  # type: ignore[misc]  # pyright: ignore[reportInconsistentConstructor]

# everything below here is defined in ast.py

_T = _TypeVar("_T", bound=AST)

if sys.version_info >= (3, 13):
    @overload
    def parse(
        source: str | ReadableBuffer,
        filename: str | ReadableBuffer | os.PathLike[Any] = "<unknown>",
        mode: Literal["exec"] = "exec",
        *,
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
        optimize: Literal[-1, 0, 1, 2] = -1,
    ) -> Module: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        filename: str | ReadableBuffer | os.PathLike[Any],
        mode: Literal["eval"],
        *,
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
        optimize: Literal[-1, 0, 1, 2] = -1,
    ) -> Expression: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        filename: str | ReadableBuffer | os.PathLike[Any],
        mode: Literal["func_type"],
        *,
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
        optimize: Literal[-1, 0, 1, 2] = -1,
    ) -> FunctionType: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        filename: str | ReadableBuffer | os.PathLike[Any],
        mode: Literal["single"],
        *,
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
        optimize: Literal[-1, 0, 1, 2] = -1,
    ) -> Interactive: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        *,
        mode: Literal["eval"],
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
        optimize: Literal[-1, 0, 1, 2] = -1,
    ) -> Expression: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        *,
        mode: Literal["func_type"],
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
        optimize: Literal[-1, 0, 1, 2] = -1,
    ) -> FunctionType: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        *,
        mode: Literal["single"],
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
        optimize: Literal[-1, 0, 1, 2] = -1,
    ) -> Interactive: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        filename: str | ReadableBuffer | os.PathLike[Any] = "<unknown>",
        mode: str = "exec",
        *,
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
        optimize: Literal[-1, 0, 1, 2] = -1,
    ) -> mod: ...

else:
    @overload
    def parse(
        source: str | ReadableBuffer,
        filename: str | ReadableBuffer | os.PathLike[Any] = "<unknown>",
        mode: Literal["exec"] = "exec",
        *,
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
    ) -> Module: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        filename: str | ReadableBuffer | os.PathLike[Any],
        mode: Literal["eval"],
        *,
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
    ) -> Expression: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        filename: str | ReadableBuffer | os.PathLike[Any],
        mode: Literal["func_type"],
        *,
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
    ) -> FunctionType: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        filename: str | ReadableBuffer | os.PathLike[Any],
        mode: Literal["single"],
        *,
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
    ) -> Interactive: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        *,
        mode: Literal["eval"],
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
    ) -> Expression: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        *,
        mode: Literal["func_type"],
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
    ) -> FunctionType: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        *,
        mode: Literal["single"],
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
    ) -> Interactive: ...
    @overload
    def parse(
        source: str | ReadableBuffer,
        filename: str | ReadableBuffer | os.PathLike[Any] = "<unknown>",
        mode: str = "exec",
        *,
        type_comments: bool = False,
        feature_version: None | int | tuple[int, int] = None,
    ) -> mod: ...

def literal_eval(node_or_string: str | AST) -> Any: ...

if sys.version_info >= (3, 13):
    def dump(
        node: AST,
        annotate_fields: bool = True,
        include_attributes: bool = False,
        *,
        indent: int | str | None = None,
        show_empty: bool = False,
    ) -> str: ...

else:
    def dump(
        node: AST, annotate_fields: bool = True, include_attributes: bool = False, *, indent: int | str | None = None
    ) -> str: ...

def copy_location(new_node: _T, old_node: AST) -> _T: ...
def fix_missing_locations(node: _T) -> _T: ...
def increment_lineno(node: _T, n: int = 1) -> _T: ...
def iter_fields(node: AST) -> Iterator[tuple[str, Any]]: ...
def iter_child_nodes(node: AST) -> Iterator[AST]: ...
def get_docstring(node: AsyncFunctionDef | FunctionDef | ClassDef | Module, clean: bool = True) -> str | None: ...
def get_source_segment(source: str, node: AST, *, padded: bool = False) -> str | None: ...
def walk(node: AST) -> Iterator[AST]: ...

if sys.version_info >= (3, 14):
    def compare(left: AST, right: AST, /, *, compare_attributes: bool = False) -> bool: ...

class NodeVisitor:
    # All visit methods below can be overwritten by subclasses and return an
    # arbitrary value, which is passed to the caller.
    def visit(self, node: AST) -> Any: ...
    def generic_visit(self, node: AST) -> Any: ...
    # The following visit methods are not defined on NodeVisitor, but can
    # be implemented by subclasses and are called during a visit if defined.
    def visit_Module(self, node: Module) -> Any: ...
    def visit_Interactive(self, node: Interactive) -> Any: ...
    def visit_Expression(self, node: Expression) -> Any: ...
    def visit_FunctionDef(self, node: FunctionDef) -> Any: ...
    def visit_AsyncFunctionDef(self, node: AsyncFunctionDef) -> Any: ...
    def visit_ClassDef(self, node: ClassDef) -> Any: ...
    def visit_Return(self, node: Return) -> Any: ...
    def visit_Delete(self, node: Delete) -> Any: ...
    def visit_Assign(self, node: Assign) -> Any: ...
    def visit_AugAssign(self, node: AugAssign) -> Any: ...
    def visit_AnnAssign(self, node: AnnAssign) -> Any: ...
    def visit_For(self, node: For) -> Any: ...
    def visit_AsyncFor(self, node: AsyncFor) -> Any: ...
    def visit_While(self, node: While) -> Any: ...
    def visit_If(self, node: If) -> Any: ...
    def visit_With(self, node: With) -> Any: ...
    def visit_AsyncWith(self, node: AsyncWith) -> Any: ...
    def visit_Raise(self, node: Raise) -> Any: ...
    def visit_Try(self, node: Try) -> Any: ...
    def visit_Assert(self, node: Assert) -> Any: ...
    def visit_Import(self, node: Import) -> Any: ...
    def visit_ImportFrom(self, node: ImportFrom) -> Any: ...
    def visit_Global(self, node: Global) -> Any: ...
    def visit_Nonlocal(self, node: Nonlocal) -> Any: ...
    def visit_Expr(self, node: Expr) -> Any: ...
    def visit_Pass(self, node: Pass) -> Any: ...
    def visit_Break(self, node: Break) -> Any: ...
    def visit_Continue(self, node: Continue) -> Any: ...
    def visit_Slice(self, node: Slice) -> Any: ...
    def visit_BoolOp(self, node: BoolOp) -> Any: ...
    def visit_BinOp(self, node: BinOp) -> Any: ...
    def visit_UnaryOp(self, node: UnaryOp) -> Any: ...
    def visit_Lambda(self, node: Lambda) -> Any: ...
    def visit_IfExp(self, node: IfExp) -> Any: ...
    def visit_Dict(self, node: Dict) -> Any: ...
    def visit_Set(self, node: Set) -> Any: ...
    def visit_ListComp(self, node: ListComp) -> Any: ...
    def visit_SetComp(self, node: SetComp) -> Any: ...
    def visit_DictComp(self, node: DictComp) -> Any: ...
    def visit_GeneratorExp(self, node: GeneratorExp) -> Any: ...
    def visit_Await(self, node: Await) -> Any: ...
    def visit_Yield(self, node: Yield) -> Any: ...
    def visit_YieldFrom(self, node: YieldFrom) -> Any: ...
    def visit_Compare(self, node: Compare) -> Any: ...
    def visit_Call(self, node: Call) -> Any: ...
    def visit_FormattedValue(self, node: FormattedValue) -> Any: ...
    def visit_JoinedStr(self, node: JoinedStr) -> Any: ...
    def visit_Constant(self, node: Constant) -> Any: ...
    def visit_NamedExpr(self, node: NamedExpr) -> Any: ...
    def visit_TypeIgnore(self, node: TypeIgnore) -> Any: ...
    def visit_Attribute(self, node: Attribute) -> Any: ...
    def visit_Subscript(self, node: Subscript) -> Any: ...
    def visit_Starred(self, node: Starred) -> Any: ...
    def visit_Name(self, node: Name) -> Any: ...
    def visit_List(self, node: List) -> Any: ...
    def visit_Tuple(self, node: Tuple) -> Any: ...
    def visit_Del(self, node: Del) -> Any: ...
    def visit_Load(self, node: Load) -> Any: ...
    def visit_Store(self, node: Store) -> Any: ...
    def visit_And(self, node: And) -> Any: ...
    def visit_Or(self, node: Or) -> Any: ...
    def visit_Add(self, node: Add) -> Any: ...
    def visit_BitAnd(self, node: BitAnd) -> Any: ...
    def visit_BitOr(self, node: BitOr) -> Any: ...
    def visit_BitXor(self, node: BitXor) -> Any: ...
    def visit_Div(self, node: Div) -> Any: ...
    def visit_FloorDiv(self, node: FloorDiv) -> Any: ...
    def visit_LShift(self, node: LShift) -> Any: ...
    def visit_Mod(self, node: Mod) -> Any: ...
    def visit_Mult(self, node: Mult) -> Any: ...
    def visit_MatMult(self, node: MatMult) -> Any: ...
    def visit_Pow(self, node: Pow) -> Any: ...
    def visit_RShift(self, node: RShift) -> Any: ...
    def visit_Sub(self, node: Sub) -> Any: ...
    def visit_Invert(self, node: Invert) -> Any: ...
    def visit_Not(self, node: Not) -> Any: ...
    def visit_UAdd(self, node: UAdd) -> Any: ...
    def visit_USub(self, node: USub) -> Any: ...
    def visit_Eq(self, node: Eq) -> Any: ...
    def visit_Gt(self, node: Gt) -> Any: ...
    def visit_GtE(self, node: GtE) -> Any: ...
    def visit_In(self, node: In) -> Any: ...
    def visit_Is(self, node: Is) -> Any: ...
    def visit_IsNot(self, node: IsNot) -> Any: ...
    def visit_Lt(self, node: Lt) -> Any: ...
    def visit_LtE(self, node: LtE) -> Any: ...
    def visit_NotEq(self, node: NotEq) -> Any: ...
    def visit_NotIn(self, node: NotIn) -> Any: ...
    def visit_comprehension(self, node: comprehension) -> Any: ...
    def visit_ExceptHandler(self, node: ExceptHandler) -> Any: ...
    def visit_arguments(self, node: arguments) -> Any: ...
    def visit_arg(self, node: arg) -> Any: ...
    def visit_keyword(self, node: keyword) -> Any: ...
    def visit_alias(self, node: alias) -> Any: ...
    def visit_withitem(self, node: withitem) -> Any: ...
    if sys.version_info >= (3, 10):
        def visit_Match(self, node: Match) -> Any: ...
        def visit_match_case(self, node: match_case) -> Any: ...
        def visit_MatchValue(self, node: MatchValue) -> Any: ...
        def visit_MatchSequence(self, node: MatchSequence) -> Any: ...
        def visit_MatchSingleton(self, node: MatchSingleton) -> Any: ...
        def visit_MatchStar(self, node: MatchStar) -> Any: ...
        def visit_MatchMapping(self, node: MatchMapping) -> Any: ...
        def visit_MatchClass(self, node: MatchClass) -> Any: ...
        def visit_MatchAs(self, node: MatchAs) -> Any: ...
        def visit_MatchOr(self, node: MatchOr) -> Any: ...

    if sys.version_info >= (3, 11):
        def visit_TryStar(self, node: TryStar) -> Any: ...

    if sys.version_info >= (3, 12):
        def visit_TypeVar(self, node: TypeVar) -> Any: ...
        def visit_ParamSpec(self, node: ParamSpec) -> Any: ...
        def visit_TypeVarTuple(self, node: TypeVarTuple) -> Any: ...
        def visit_TypeAlias(self, node: TypeAlias) -> Any: ...

    # visit methods for deprecated nodes
    def visit_ExtSlice(self, node: ExtSlice) -> Any: ...
    def visit_Index(self, node: Index) -> Any: ...
    def visit_Suite(self, node: Suite) -> Any: ...
    def visit_AugLoad(self, node: AugLoad) -> Any: ...
    def visit_AugStore(self, node: AugStore) -> Any: ...
    def visit_Param(self, node: Param) -> Any: ...

    if sys.version_info < (3, 14):
        @deprecated("Replaced by visit_Constant; removed in Python 3.14")
        def visit_Num(self, node: Num) -> Any: ...  # type: ignore[deprecated]
        @deprecated("Replaced by visit_Constant; removed in Python 3.14")
        def visit_Str(self, node: Str) -> Any: ...  # type: ignore[deprecated]
        @deprecated("Replaced by visit_Constant; removed in Python 3.14")
        def visit_Bytes(self, node: Bytes) -> Any: ...  # type: ignore[deprecated]
        @deprecated("Replaced by visit_Constant; removed in Python 3.14")
        def visit_NameConstant(self, node: NameConstant) -> Any: ...  # type: ignore[deprecated]
        @deprecated("Replaced by visit_Constant; removed in Python 3.14")
        def visit_Ellipsis(self, node: Ellipsis) -> Any: ...  # type: ignore[deprecated]

class NodeTransformer(NodeVisitor):
    def generic_visit(self, node: AST) -> AST: ...
    # TODO: Override the visit_* methods with better return types.
    #       The usual return type is AST | None, but Iterable[AST]
    #       is also allowed in some cases -- this needs to be mapped.

def unparse(ast_obj: AST) -> str: ...

if sys.version_info >= (3, 14):
    def main(args: Sequence[str] | None = None) -> None: ...

else:
    def main() -> None: ...
