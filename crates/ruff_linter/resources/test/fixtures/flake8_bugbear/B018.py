class Foo1:
    """abc"""


class Foo2:
    """abc"""

    a = 2
    "str"  # Str (no raise)
    f"{int}"  # JoinedStr (no raise)
    1j  # Number (complex)
    1  # Number (int)
    1.0  # Number (float)
    b"foo"  # Binary
    True  # NameConstant (True)
    False  # NameConstant (False)
    None  # NameConstant (None)
    [1, 2]  # list
    {1, 2}  # set
    {"foo": "bar"}  # dict


class Foo3:
    123
    a = 2
    "str"
    1


def foo1():
    """my docstring"""


def foo2():
    """my docstring"""
    a = 2
    "str"  # Str (no raise)
    f"{int}"  # JoinedStr (no raise)
    1j  # Number (complex)
    1  # Number (int)
    1.0  # Number (float)
    b"foo"  # Binary
    True  # NameConstant (True)
    False  # NameConstant (False)
    None  # NameConstant (None)
    [1, 2]  # list
    {1, 2}  # set
    {"foo": "bar"}  # dict


def foo3():
    123
    a = 2
    "str"
    3


def foo4():
    ...


def foo5():
    foo.bar  # Attribute (raise)
    object().__class__  # Attribute (raise)
    "foo" + "bar"  # BinOp (raise)


def foo6():
    # Bare tuples: the wrapping tuple is useless even when elements have side effects.
    # See https://github.com/astral-sh/ruff/issues/14131
    foo(),  # single-element trailing-comma tuple wrapping a call
    foo(), bar()  # multi-element tuple wrapping calls
    1, 2, 3  # tuple of literals
    (1, 2, 3)  # parenthesized tuple of literals
