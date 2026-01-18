from __future__ import annotations

MESSAGE = (
    "Logging message should be a string literal with parameterized arguments; "
    "avoid runtime string interpolation. (RLI001)"
)


def _find_message_argument(arguments):
    for argument in arguments:
        if argument.kind == "positional" and not argument.is_unpack:
            return argument
    for argument in arguments:
        if argument.kind == "keyword" and not argument.is_unpack:
            name = argument.name
            if name is not None and name.lower() == "msg":
                return argument
    return None


def _is_dynamic_message(argument):
    if argument.is_string_literal:
        return False

    if argument.is_fstring:
        return True

    if argument.expr_kind == "BinOp":
        operator = argument.binop_operator
        if operator in {"+", "%"}:
            return True

    if argument.expr_kind == "Call":
        callee = argument.call_function_text
        if isinstance(callee, str) and callee.lower().endswith(".format"):
            return True

    return False


def check_expr(node, ctx):
    if not node.arguments:
        return

    message_argument = _find_message_argument(node.arguments)
    if message_argument is None:
        return

    if _is_dynamic_message(message_argument):
        ctx.report(MESSAGE, message_argument.span)
