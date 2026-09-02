# https://github.com/astral-sh/ruff/issues/18807

f"module docstring"

a = 1
f"attribute docstring"

b: int = 2
f"annotated attribute docstring"

c = d = 3
f"not an attribute docstring: multiple targets"

e, g = 4, 5
f"not an attribute docstring: tuple target"

y = f"not a docstring: assigned"


def fn():
    f"function docstring"

    i = 1
    f"not an attribute docstring: inside a function body"


class C:
    f"class docstring"

    h = 1
    f"attribute docstring in a class body"


def nested():
    x = 1
    print(f"not a docstring: nested in a call")
