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
    x["item"]  # Subscript (raise)
    obj.attr["key"]  # Subscript (raise)
    get_dict()["key"]  # Subscript on call result (raise)
    x[0]  # Subscript with int key (raise)
    _ = x["item"]  # OK: result intentionally ignored
