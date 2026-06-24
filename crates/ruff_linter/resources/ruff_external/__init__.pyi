from __future__ import annotations

from typing import Any, Callable, Mapping, Optional, Tuple

Span = Tuple[int, int]
Reporter = Callable[[str, Optional[Span]], None]

__all__ = ["Context", "Node", "RawNode"]


class Node:
    _kind: str
    _span: Span
    _text: str
    _repr: str
    node_id: int

    def __init__(self, kind: str, span: Span, text: str, repr_value: str, node_id: int) -> None: ...

    def __getitem__(self, key: str) -> Any: ...

    def get(self, key: str, default: Any = ...) -> Any: ...


class RawNode(Node):
    pass


class Context:
    code: str
    name: str
    config: Mapping[str, Any]
    _report: Reporter

    def __init__(self, code: str, name: str, config: Mapping[str, Any], reporter: Reporter) -> None: ...

    def report(self, message: str, span: Optional[Span] = ...) -> None: ...

from .nodes import *
from .nodes import __all__ as _node_all

__all__ = __all__ + _node_all
