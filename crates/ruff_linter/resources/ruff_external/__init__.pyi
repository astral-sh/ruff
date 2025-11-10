from __future__ import annotations

from typing import Any, Callable, Iterable, Optional, Tuple

Span = Tuple[int, int]
Reporter = Callable[[str, Optional[Span]], None]

__all__ = ["Context", "Node", "CallArgument"]


class Node:
    _kind: str
    _span: Span
    _text: str
    _repr: str
    _callee: Optional[str]
    function_text: Optional[str]
    function_kind: Optional[str]
    arguments: Tuple["CallArgument", ...]

    def __init__(
        self,
        kind: str,
        span: Span,
        text: str,
        repr_value: str,
        callee: Optional[str] = ...,
        function_text: Optional[str] = ...,
        function_kind: Optional[str] = ...,
        arguments: Optional[Iterable["CallArgument"]] = ...,
    ) -> None: ...

    def __getitem__(self, key: str) -> Any: ...

    def get(self, key: str, default: Any = ...) -> Any: ...


class CallArgument:
    kind: str
    is_unpack: bool
    name: Optional[str]
    span: Span
    expr_kind: str
    is_string_literal: bool
    is_fstring: bool
    binop_operator: Optional[str]
    call_function_text: Optional[str]

    def __init__(
        self,
        kind: str,
        is_unpack: bool,
        span: Span,
        expr_kind: str,
        is_string_literal: bool,
        is_fstring: bool,
        binop_operator: Optional[str] = ...,
        call_function_text: Optional[str] = ...,
        name: Optional[str] = ...,
    ) -> None: ...

    def __getitem__(self, key: str) -> Any: ...

    def get(self, key: str, default: Any = ...) -> Any: ...


class Context:
    code: str
    name: str
    _report: Reporter

    def __init__(self, code: str, name: str, reporter: Reporter) -> None: ...

    def report(self, message: str, span: Optional[Span] = ...) -> None: ...
