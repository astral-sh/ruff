###
# Errors — fixable on Python 3.12+ thanks to PEP 701, which lifts the
# restriction that interpolations cannot reuse the outer quote character
# and allows multi-line expressions inside `{...}`.
# Refer: https://github.com/astral-sh/ruff/issues/2031
###

# String-literal argument (interpolation reuses outer quote).
"Hello {}".format("world")

# Subscript with a string-literal key.
"Magic wand: {}".format(bag["wand"])

# `BinOp` argument that contains a string literal.
"{}".format(len(l) * "─")

# Multi-line argument.
"{}".format(
    [
        1,
        2,
        3,
    ]
)

# Multi-line keyword argument.
"{a}".format(
    a=[
        1,
        2,
        3,
    ]
)
