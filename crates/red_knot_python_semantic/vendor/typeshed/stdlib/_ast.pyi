import sys
import typing_extensions
from typing import Any, ClassVar, Generic, Literal, TypedDict, overload
from typing_extensions import Unpack

PyCF_ONLY_AST: Literal[1024]
PyCF_TYPE_COMMENTS: Literal[4096]
PyCF_ALLOW_TOP_LEVEL_AWAIT: Literal[8192]

if sys.version_info >= (3, 13):
    PyCF_OPTIMIZED_AST: Literal[33792]

# Used for node end positions in constructor keyword arguments
_EndPositionT = typing_extensions.TypeVar("_EndPositionT", int, int | None, default=int | None)

# Alias used for fields that must always be valid identifiers
# A string `x` counts as a valid identifier if both the following are True
# (1) `x.isidentifier()` evaluates to `True`
# (2) `keyword.iskeyword(x)` evaluates to `False`
_Identifier: typing_extensions.TypeAlias = str

# Corresponds to the names in the `_attributes` class variable which is non-empty in certain AST nodes
class _Attributes(TypedDict, Generic[_EndPositionT], total=False):
    lineno: int
    col_offset: int
    end_lineno: _EndPositionT
    end_col_offset: _EndPositionT

class AST:
    if sys.version_info >= (3, 10):
        __match_args__ = ()
    _attributes: ClassVar[tuple[str, ...]]
    _fields: ClassVar[tuple[str, ...]]
    if sys.version_info >= (3, 13):
        _field_types: ClassVar[dict[str, Any]]

class mod(AST): ...
class type_ignore(AST): ...

class TypeIgnore(type_ignore):
    if sys.version_info >= (3, 10):
        __match_args__ = ("lineno", "tag")
    lineno: int
    tag: str
    def __init__(self, lineno: int, tag: str) -> None: ...

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

class Module(mod):
    if sys.version_info >= (3, 10):
        __match_args__ = ("body", "type_ignores")
    body: list[stmt]
    type_ignores: list[TypeIgnore]
    if sys.version_info >= (3, 13):
        def __init__(self, body: list[stmt] = ..., type_ignores: list[TypeIgnore] = ...) -> None: ...
    else:
        def __init__(self, body: list[stmt], type_ignores: list[TypeIgnore]) -> None: ...

class Interactive(mod):
    if sys.version_info >= (3, 10):
        __match_args__ = ("body",)
    body: list[stmt]
    if sys.version_info >= (3, 13):
        def __init__(self, body: list[stmt] = ...) -> None: ...
    else:
        def __init__(self, body: list[stmt]) -> None: ...

class Expression(mod):
    if sys.version_info >= (3, 10):
        __match_args__ = ("body",)
    body: expr
    def __init__(self, body: expr) -> None: ...

class stmt(AST):
    lineno: int
    col_offset: int
    end_lineno: int | None
    end_col_offset: int | None
    def __init__(self, **kwargs: Unpack[_Attributes]) -> None: ...

class FunctionDef(stmt):
    if sys.version_info >= (3, 12):
        __match_args__ = ("name", "args", "body", "decorator_list", "returns", "type_comment", "type_params")
    elif sys.version_info >= (3, 10):
        __match_args__ = ("name", "args", "body", "decorator_list", "returns", "type_comment")
    name: _Identifier
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
            name: _Identifier,
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
            name: _Identifier,
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
            name: _Identifier,
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
            name: _Identifier,
            args: arguments,
            body: list[stmt],
            decorator_list: list[expr],
            returns: expr | None = None,
            type_comment: str | None = None,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...

class AsyncFunctionDef(stmt):
    if sys.version_info >= (3, 12):
        __match_args__ = ("name", "args", "body", "decorator_list", "returns", "type_comment", "type_params")
    elif sys.version_info >= (3, 10):
        __match_args__ = ("name", "args", "body", "decorator_list", "returns", "type_comment")
    name: _Identifier
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
            name: _Identifier,
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
            name: _Identifier,
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
            name: _Identifier,
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
            name: _Identifier,
            args: arguments,
            body: list[stmt],
            decorator_list: list[expr],
            returns: expr | None = None,
            type_comment: str | None = None,
            **kwargs: Unpack[_Attributes],
        ) -> None: ...

class ClassDef(stmt):
    if sys.version_info >= (3, 12):
        __match_args__ = ("name", "bases", "keywords", "body", "decorator_list", "type_params")
    elif sys.version_info >= (3, 10):
        __match_args__ = ("name", "bases", "keywords", "body", "decorator_list")
    name: _Identifier
    bases: list[expr]
    keywords: list[keyword]
    body: list[stmt]
    decorator_list: list[expr]
    if sys.version_info >= (3, 12):
        type_params: list[type_param]
    if sys.version_info >= (3, 13):
        def __init__(
            self,
            name: _Identifier,
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
            name: _Identifier,
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
            name: _Identifier,
            bases: list[expr],
            keywords: list[keyword],
            body: list[stmt],
            decorator_list: list[expr],
            **kwargs: Unpack[_Attributes],
        ) -> None: ...

class Return(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value",)
    value: expr | None
    def __init__(self, value: expr | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

class Delete(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("targets",)
    targets: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, targets: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, targets: list[expr], **kwargs: Unpack[_Attributes]) -> None: ...

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

class AugAssign(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("target", "op", "value")
    target: Name | Attribute | Subscript
    op: operator
    value: expr
    def __init__(
        self, target: Name | Attribute | Subscript, op: operator, value: expr, **kwargs: Unpack[_Attributes]
    ) -> None: ...

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

class Raise(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("exc", "cause")
    exc: expr | None
    cause: expr | None
    def __init__(self, exc: expr | None = None, cause: expr | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

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

class Assert(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("test", "msg")
    test: expr
    msg: expr | None
    def __init__(self, test: expr, msg: expr | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

class Import(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("names",)
    names: list[alias]
    if sys.version_info >= (3, 13):
        def __init__(self, names: list[alias] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, names: list[alias], **kwargs: Unpack[_Attributes]) -> None: ...

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

class Global(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("names",)
    names: list[_Identifier]
    if sys.version_info >= (3, 13):
        def __init__(self, names: list[_Identifier] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, names: list[_Identifier], **kwargs: Unpack[_Attributes]) -> None: ...

class Nonlocal(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("names",)
    names: list[_Identifier]
    if sys.version_info >= (3, 13):
        def __init__(self, names: list[_Identifier] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, names: list[_Identifier], **kwargs: Unpack[_Attributes]) -> None: ...

class Expr(stmt):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value",)
    value: expr
    def __init__(self, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...

class Pass(stmt): ...
class Break(stmt): ...
class Continue(stmt): ...

class expr(AST):
    lineno: int
    col_offset: int
    end_lineno: int | None
    end_col_offset: int | None
    def __init__(self, **kwargs: Unpack[_Attributes]) -> None: ...

class BoolOp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("op", "values")
    op: boolop
    values: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, op: boolop, values: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, op: boolop, values: list[expr], **kwargs: Unpack[_Attributes]) -> None: ...

class BinOp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("left", "op", "right")
    left: expr
    op: operator
    right: expr
    def __init__(self, left: expr, op: operator, right: expr, **kwargs: Unpack[_Attributes]) -> None: ...

class UnaryOp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("op", "operand")
    op: unaryop
    operand: expr
    def __init__(self, op: unaryop, operand: expr, **kwargs: Unpack[_Attributes]) -> None: ...

class Lambda(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("args", "body")
    args: arguments
    body: expr
    def __init__(self, args: arguments, body: expr, **kwargs: Unpack[_Attributes]) -> None: ...

class IfExp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("test", "body", "orelse")
    test: expr
    body: expr
    orelse: expr
    def __init__(self, test: expr, body: expr, orelse: expr, **kwargs: Unpack[_Attributes]) -> None: ...

class Dict(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("keys", "values")
    keys: list[expr | None]
    values: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, keys: list[expr | None] = ..., values: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, keys: list[expr | None], values: list[expr], **kwargs: Unpack[_Attributes]) -> None: ...

class Set(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elts",)
    elts: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, elts: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elts: list[expr], **kwargs: Unpack[_Attributes]) -> None: ...

class ListComp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elt", "generators")
    elt: expr
    generators: list[comprehension]
    if sys.version_info >= (3, 13):
        def __init__(self, elt: expr, generators: list[comprehension] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elt: expr, generators: list[comprehension], **kwargs: Unpack[_Attributes]) -> None: ...

class SetComp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elt", "generators")
    elt: expr
    generators: list[comprehension]
    if sys.version_info >= (3, 13):
        def __init__(self, elt: expr, generators: list[comprehension] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elt: expr, generators: list[comprehension], **kwargs: Unpack[_Attributes]) -> None: ...

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

class GeneratorExp(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elt", "generators")
    elt: expr
    generators: list[comprehension]
    if sys.version_info >= (3, 13):
        def __init__(self, elt: expr, generators: list[comprehension] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elt: expr, generators: list[comprehension], **kwargs: Unpack[_Attributes]) -> None: ...

class Await(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value",)
    value: expr
    def __init__(self, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...

class Yield(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value",)
    value: expr | None
    def __init__(self, value: expr | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

class YieldFrom(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value",)
    value: expr
    def __init__(self, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...

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

class FormattedValue(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value", "conversion", "format_spec")
    value: expr
    conversion: int
    format_spec: expr | None
    def __init__(self, value: expr, conversion: int, format_spec: expr | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

class JoinedStr(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("values",)
    values: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, values: list[expr] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, values: list[expr], **kwargs: Unpack[_Attributes]) -> None: ...

class Constant(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value", "kind")
    value: Any  # None, str, bytes, bool, int, float, complex, Ellipsis
    kind: str | None
    if sys.version_info < (3, 14):
        # Aliases for value, for backwards compatibility
        s: Any
        n: int | float | complex

    def __init__(self, value: Any, kind: str | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

class NamedExpr(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("target", "value")
    target: Name
    value: expr
    def __init__(self, target: Name, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...

class Attribute(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value", "attr", "ctx")
    value: expr
    attr: _Identifier
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    def __init__(self, value: expr, attr: _Identifier, ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

if sys.version_info >= (3, 9):
    _Slice: typing_extensions.TypeAlias = expr
    _SliceAttributes: typing_extensions.TypeAlias = _Attributes
else:
    class slice(AST): ...
    _Slice: typing_extensions.TypeAlias = slice

    class _SliceAttributes(TypedDict): ...

class Slice(_Slice):
    if sys.version_info >= (3, 10):
        __match_args__ = ("lower", "upper", "step")
    lower: expr | None
    upper: expr | None
    step: expr | None
    def __init__(
        self, lower: expr | None = None, upper: expr | None = None, step: expr | None = None, **kwargs: Unpack[_SliceAttributes]
    ) -> None: ...

if sys.version_info < (3, 9):
    class ExtSlice(slice):
        dims: list[slice]
        def __init__(self, dims: list[slice], **kwargs: Unpack[_SliceAttributes]) -> None: ...

    class Index(slice):
        value: expr
        def __init__(self, value: expr, **kwargs: Unpack[_SliceAttributes]) -> None: ...

class Subscript(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value", "slice", "ctx")
    value: expr
    slice: _Slice
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    def __init__(self, value: expr, slice: _Slice, ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

class Starred(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("value", "ctx")
    value: expr
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    def __init__(self, value: expr, ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

class Name(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("id", "ctx")
    id: _Identifier
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    def __init__(self, id: _Identifier, ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

class List(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elts", "ctx")
    elts: list[expr]
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    if sys.version_info >= (3, 13):
        def __init__(self, elts: list[expr] = ..., ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elts: list[expr], ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

class Tuple(expr):
    if sys.version_info >= (3, 10):
        __match_args__ = ("elts", "ctx")
    elts: list[expr]
    ctx: expr_context  # Not present in Python < 3.13 if not passed to `__init__`
    if sys.version_info >= (3, 9):
        dims: list[expr]
    if sys.version_info >= (3, 13):
        def __init__(self, elts: list[expr] = ..., ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...
    else:
        def __init__(self, elts: list[expr], ctx: expr_context = ..., **kwargs: Unpack[_Attributes]) -> None: ...

class expr_context(AST): ...

if sys.version_info < (3, 9):
    class AugLoad(expr_context): ...
    class AugStore(expr_context): ...
    class Param(expr_context): ...

    class Suite(mod):
        body: list[stmt]
        def __init__(self, body: list[stmt]) -> None: ...

class Del(expr_context): ...
class Load(expr_context): ...
class Store(expr_context): ...
class boolop(AST): ...
class And(boolop): ...
class Or(boolop): ...
class operator(AST): ...
class Add(operator): ...
class BitAnd(operator): ...
class BitOr(operator): ...
class BitXor(operator): ...
class Div(operator): ...
class FloorDiv(operator): ...
class LShift(operator): ...
class Mod(operator): ...
class Mult(operator): ...
class MatMult(operator): ...
class Pow(operator): ...
class RShift(operator): ...
class Sub(operator): ...
class unaryop(AST): ...
class Invert(unaryop): ...
class Not(unaryop): ...
class UAdd(unaryop): ...
class USub(unaryop): ...
class cmpop(AST): ...
class Eq(cmpop): ...
class Gt(cmpop): ...
class GtE(cmpop): ...
class In(cmpop): ...
class Is(cmpop): ...
class IsNot(cmpop): ...
class Lt(cmpop): ...
class LtE(cmpop): ...
class NotEq(cmpop): ...
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

class excepthandler(AST):
    lineno: int
    col_offset: int
    end_lineno: int | None
    end_col_offset: int | None
    def __init__(self, **kwargs: Unpack[_Attributes]) -> None: ...

class ExceptHandler(excepthandler):
    if sys.version_info >= (3, 10):
        __match_args__ = ("type", "name", "body")
    type: expr | None
    name: _Identifier | None
    body: list[stmt]
    if sys.version_info >= (3, 13):
        def __init__(
            self, type: expr | None = None, name: _Identifier | None = None, body: list[stmt] = ..., **kwargs: Unpack[_Attributes]
        ) -> None: ...
    else:
        @overload
        def __init__(
            self, type: expr | None, name: _Identifier | None, body: list[stmt], **kwargs: Unpack[_Attributes]
        ) -> None: ...
        @overload
        def __init__(
            self, type: expr | None = None, name: _Identifier | None = None, *, body: list[stmt], **kwargs: Unpack[_Attributes]
        ) -> None: ...

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

class arg(AST):
    lineno: int
    col_offset: int
    end_lineno: int | None
    end_col_offset: int | None
    if sys.version_info >= (3, 10):
        __match_args__ = ("arg", "annotation", "type_comment")
    arg: _Identifier
    annotation: expr | None
    type_comment: str | None
    def __init__(
        self, arg: _Identifier, annotation: expr | None = None, type_comment: str | None = None, **kwargs: Unpack[_Attributes]
    ) -> None: ...

class keyword(AST):
    lineno: int
    col_offset: int
    end_lineno: int | None
    end_col_offset: int | None
    if sys.version_info >= (3, 10):
        __match_args__ = ("arg", "value")
    arg: _Identifier | None
    value: expr
    @overload
    def __init__(self, arg: _Identifier | None, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...
    @overload
    def __init__(self, arg: _Identifier | None = None, *, value: expr, **kwargs: Unpack[_Attributes]) -> None: ...

class alias(AST):
    lineno: int
    col_offset: int
    end_lineno: int | None
    end_col_offset: int | None
    if sys.version_info >= (3, 10):
        __match_args__ = ("name", "asname")
    name: str
    asname: _Identifier | None
    def __init__(self, name: str, asname: _Identifier | None = None, **kwargs: Unpack[_Attributes]) -> None: ...

class withitem(AST):
    if sys.version_info >= (3, 10):
        __match_args__ = ("context_expr", "optional_vars")
    context_expr: expr
    optional_vars: expr | None
    def __init__(self, context_expr: expr, optional_vars: expr | None = None) -> None: ...

if sys.version_info >= (3, 10):
    class Match(stmt):
        __match_args__ = ("subject", "cases")
        subject: expr
        cases: list[match_case]
        if sys.version_info >= (3, 13):
            def __init__(self, subject: expr, cases: list[match_case] = ..., **kwargs: Unpack[_Attributes]) -> None: ...
        else:
            def __init__(self, subject: expr, cases: list[match_case], **kwargs: Unpack[_Attributes]) -> None: ...

    class pattern(AST):
        lineno: int
        col_offset: int
        end_lineno: int
        end_col_offset: int
        def __init__(self, **kwargs: Unpack[_Attributes[int]]) -> None: ...

    # Without the alias, Pyright complains variables named pattern are recursively defined
    _Pattern: typing_extensions.TypeAlias = pattern

    class match_case(AST):
        __match_args__ = ("pattern", "guard", "body")
        pattern: _Pattern
        guard: expr | None
        body: list[stmt]
        if sys.version_info >= (3, 13):
            def __init__(self, pattern: _Pattern, guard: expr | None = None, body: list[stmt] = ...) -> None: ...
        else:
            @overload
            def __init__(self, pattern: _Pattern, guard: expr | None, body: list[stmt]) -> None: ...
            @overload
            def __init__(self, pattern: _Pattern, guard: expr | None = None, *, body: list[stmt]) -> None: ...

    class MatchValue(pattern):
        __match_args__ = ("value",)
        value: expr
        def __init__(self, value: expr, **kwargs: Unpack[_Attributes[int]]) -> None: ...

    class MatchSingleton(pattern):
        __match_args__ = ("value",)
        value: Literal[True, False] | None
        def __init__(self, value: Literal[True, False] | None, **kwargs: Unpack[_Attributes[int]]) -> None: ...

    class MatchSequence(pattern):
        __match_args__ = ("patterns",)
        patterns: list[pattern]
        if sys.version_info >= (3, 13):
            def __init__(self, patterns: list[pattern] = ..., **kwargs: Unpack[_Attributes[int]]) -> None: ...
        else:
            def __init__(self, patterns: list[pattern], **kwargs: Unpack[_Attributes[int]]) -> None: ...

    class MatchStar(pattern):
        __match_args__ = ("name",)
        name: _Identifier | None
        def __init__(self, name: _Identifier | None, **kwargs: Unpack[_Attributes[int]]) -> None: ...

    class MatchMapping(pattern):
        __match_args__ = ("keys", "patterns", "rest")
        keys: list[expr]
        patterns: list[pattern]
        rest: _Identifier | None
        if sys.version_info >= (3, 13):
            def __init__(
                self,
                keys: list[expr] = ...,
                patterns: list[pattern] = ...,
                rest: _Identifier | None = None,
                **kwargs: Unpack[_Attributes[int]],
            ) -> None: ...
        else:
            def __init__(
                self,
                keys: list[expr],
                patterns: list[pattern],
                rest: _Identifier | None = None,
                **kwargs: Unpack[_Attributes[int]],
            ) -> None: ...

    class MatchClass(pattern):
        __match_args__ = ("cls", "patterns", "kwd_attrs", "kwd_patterns")
        cls: expr
        patterns: list[pattern]
        kwd_attrs: list[_Identifier]
        kwd_patterns: list[pattern]
        if sys.version_info >= (3, 13):
            def __init__(
                self,
                cls: expr,
                patterns: list[pattern] = ...,
                kwd_attrs: list[_Identifier] = ...,
                kwd_patterns: list[pattern] = ...,
                **kwargs: Unpack[_Attributes[int]],
            ) -> None: ...
        else:
            def __init__(
                self,
                cls: expr,
                patterns: list[pattern],
                kwd_attrs: list[_Identifier],
                kwd_patterns: list[pattern],
                **kwargs: Unpack[_Attributes[int]],
            ) -> None: ...

    class MatchAs(pattern):
        __match_args__ = ("pattern", "name")
        pattern: _Pattern | None
        name: _Identifier | None
        def __init__(
            self, pattern: _Pattern | None = None, name: _Identifier | None = None, **kwargs: Unpack[_Attributes[int]]
        ) -> None: ...

    class MatchOr(pattern):
        __match_args__ = ("patterns",)
        patterns: list[pattern]
        if sys.version_info >= (3, 13):
            def __init__(self, patterns: list[pattern] = ..., **kwargs: Unpack[_Attributes[int]]) -> None: ...
        else:
            def __init__(self, patterns: list[pattern], **kwargs: Unpack[_Attributes[int]]) -> None: ...

if sys.version_info >= (3, 12):
    class type_param(AST):
        lineno: int
        col_offset: int
        end_lineno: int
        end_col_offset: int
        def __init__(self, **kwargs: Unpack[_Attributes[int]]) -> None: ...

    class TypeVar(type_param):
        if sys.version_info >= (3, 13):
            __match_args__ = ("name", "bound", "default_value")
        else:
            __match_args__ = ("name", "bound")
        name: _Identifier
        bound: expr | None
        if sys.version_info >= (3, 13):
            default_value: expr | None
            def __init__(
                self,
                name: _Identifier,
                bound: expr | None = None,
                default_value: expr | None = None,
                **kwargs: Unpack[_Attributes[int]],
            ) -> None: ...
        else:
            def __init__(self, name: _Identifier, bound: expr | None = None, **kwargs: Unpack[_Attributes[int]]) -> None: ...

    class ParamSpec(type_param):
        if sys.version_info >= (3, 13):
            __match_args__ = ("name", "default_value")
        else:
            __match_args__ = ("name",)
        name: _Identifier
        if sys.version_info >= (3, 13):
            default_value: expr | None
            def __init__(
                self, name: _Identifier, default_value: expr | None = None, **kwargs: Unpack[_Attributes[int]]
            ) -> None: ...
        else:
            def __init__(self, name: _Identifier, **kwargs: Unpack[_Attributes[int]]) -> None: ...

    class TypeVarTuple(type_param):
        if sys.version_info >= (3, 13):
            __match_args__ = ("name", "default_value")
        else:
            __match_args__ = ("name",)
        name: _Identifier
        if sys.version_info >= (3, 13):
            default_value: expr | None
            def __init__(
                self, name: _Identifier, default_value: expr | None = None, **kwargs: Unpack[_Attributes[int]]
            ) -> None: ...
        else:
            def __init__(self, name: _Identifier, **kwargs: Unpack[_Attributes[int]]) -> None: ...

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
