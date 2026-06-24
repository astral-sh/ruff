from __future__ import annotations

import ruff_external as ast

MESSAGE = (
    "Logging message should be a string literal with parameterized arguments; "
    "avoid runtime string interpolation. (RLI001)"
)


def _iter_positional_args(arguments):
    for argument in arguments.args:
        match argument:
            case ast.StarredExpr(_, _):
                continue
            case _:
                yield argument


def _find_message_argument(arguments):
    for argument in _iter_positional_args(arguments):
        return argument

    for keyword in arguments.keywords:
        match keyword:
            case ast.Keyword(str() as name, value) if name.lower() == "msg":
                return value

    return None


def _is_dynamic_message(argument):
    match argument:
        case ast.StringLiteralExpr(_):
            return False
        case ast.FStringExpr(_) | ast.BinOpExpr(_, _, _):
            return True
        case ast.CallExpr(_, _):
            callee = argument.function_text
            return isinstance(callee, str) and callee.lower().endswith(".format")
        case _:
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
