"""Helpers exposed to Ruff external linters at runtime."""

__all__ = ["Context", "Node", "CallArgument"]


class Node:
    """Minimal representation of a Ruff AST node."""

    __slots__ = (
        "_kind",
        "_span",
        "_text",
        "_repr",
        "_callee",
        "function_text",
        "function_kind",
        "arguments",
    )

    def __init__(
        self,
        kind,
        span,
        text,
        repr_value,
        callee=None,
        function_text=None,
        function_kind=None,
        arguments=(),
    ):
        self._kind = kind
        self._span = span
        self._text = text
        self._repr = repr_value
        self._callee = callee
        self.function_text = function_text
        self.function_kind = function_kind
        self.arguments = arguments

    def __repr__(self):
        return (
            f"Node(kind={self._kind!r}, span={self._span!r}, callee={self._callee!r}, "
            f"function_text={self.function_text!r})"
        )

    def __getitem__(self, key):
        try:
            return getattr(self, key)
        except AttributeError as exc:
            raise KeyError(key) from exc

    def get(self, key, default=None):
        try:
            return self[key]
        except KeyError:
            return default


class CallArgument:
    """Structured view of an argument passed to a call expression."""

    __slots__ = (
        "kind",
        "is_unpack",
        "name",
        "span",
        "expr_kind",
        "is_string_literal",
        "is_fstring",
        "binop_operator",
        "call_function_text",
    )

    def __init__(
        self,
        kind,
        is_unpack,
        name,
        span,
        expr_kind,
        is_string_literal,
        is_fstring,
        binop_operator,
        call_function_text,
    ):
        self.kind = kind
        self.is_unpack = is_unpack
        self.name = name
        self.span = span
        self.expr_kind = expr_kind
        self.is_string_literal = is_string_literal
        self.is_fstring = is_fstring
        self.binop_operator = binop_operator
        self.call_function_text = call_function_text

    def __repr__(self):
        return (
            "CallArgument(kind={!r}, name={!r}, expr_kind={!r})".format(
                self.kind, self.name, self.expr_kind
            )
        )

    def __getitem__(self, key):
        try:
            return getattr(self, key)
        except AttributeError as exc:
            raise KeyError(key) from exc

    def get(self, key, default=None):
        try:
            return self[key]
        except KeyError:
            return default


class Context:
    """Execution context provided to external lint rules."""

    __slots__ = ("code", "name", "_report")

    def __init__(self, code, name, reporter):
        self.code = code
        self.name = name
        self._report = reporter

    def report(self, message, span=None):
        """Report a diagnostic emitted by the current rule."""
        self._report(message, span)

    def __repr__(self):
        return f"Context(code={self.code!r}, name={self.name!r})"
