{x: NotImplemented for x in "XY"}


# Builtin bindings are placed at top of file, but should not count as
# an "expression defined within the comprehension". So the above
# should trigger C420
# See https://github.com/astral-sh/ruff/issues/15830
