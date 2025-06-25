class A:
    ...


class A(metaclass=type):
    ...


class A(
    metaclass=type
):
    ...


class A(
    metaclass=type
    #
):
    ...


class A(
    #
    metaclass=type
):
    ...


class A(
    metaclass=type,
    #
):
    ...


class A(
    #
    metaclass=type,
    #
):
    ...


class B(A, metaclass=type):
    ...


class B(
    A,
    metaclass=type,
):
    ...


class B(
    A,
    # comment
    metaclass=type,
):
    ...


def foo():
    class A(metaclass=type):
        ...


class A(
    metaclass=type  # comment
    ,
):
    ...


type = str

class Foo(metaclass=type):
    ...


import builtins

class A(metaclass=builtins.type):
    ...