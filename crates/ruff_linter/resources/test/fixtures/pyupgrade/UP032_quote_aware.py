###
# Errors — fixable on Python 3.11 (pre-PEP 701) when the argument's quote
# does not collide with the outer string's quote.
# Refer: https://github.com/astral-sh/ruff/issues/2031
###

# Outer single quote, inner double quote — no collision.
'Magic wand: {}'.format(bag["wand"])

# `BinOp` containing a string literal whose quote does not collide.
'{}'.format(len(l) * "─")

# String-literal argument with non-colliding quote.
'Hello {}'.format("world")

###
# Non-errors on this target — the inner quote would collide with the outer.
###

"Magic wand: {}".format(bag["wand"])

"{}".format(len(l) * "─")

"Hello {}".format("world")

# Multi-line interpolation is invalid pre-PEP 701 regardless of quote choice.
'{}'.format(
    [
        1,
        2,
        3,
    ]
)
