from __future__ import annotations

import ruff_external as ast

MESSAGE = (
    "Logging message should be a string literal with parameterized arguments; "
    "avoid runtime string interpolation. (RLI001)"
)


def _iter_positional_args(arguments):
    for argument in arguments.args:
        if _is_unpack_expr(argument):
            continue
        yield argument


def _iter_keyword_args(arguments):
    for keyword in arguments.keywords:
        name = keyword.arg
        if name is None:
            continue
        yield name, keyword.value


def _find_message_argument(arguments):
    for argument in _iter_positional_args(arguments):
        return argument

    for name, value in _iter_keyword_args(arguments):
        if isinstance(name, str) and name.lower() == "msg":
            return value

    return None


def _is_unpack_expr(node):
    return isinstance(node, ast.StarredExpr)


def _is_dynamic_message(argument):
    if isinstance(argument, ast.StringLiteralExpr):
        return False

    if isinstance(argument, (ast.FStringExpr, ast.BinOpExpr)):
        return True

    if isinstance(argument, ast.CallExpr):
        callee = argument.function_text
        if isinstance(callee, str) and callee.lower().endswith(".format"):
            return True

    return False


def check_expr(node, ctx):
    arguments = node.arguments
    if arguments is None:
        return

    message_argument = _find_message_argument(arguments)
    if message_argument is None:
        return

    if _is_dynamic_message(message_argument):
        ctx.report(MESSAGE, message_argument._span)
