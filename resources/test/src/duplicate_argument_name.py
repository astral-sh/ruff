from baz.foo.if_tuple import f


def foo(x: int, y: int, x: int) -> None:
    pass


def bar(x: int, y: int, *, x: int) -> None:
    pass


def baz(x: int, y: int, **x: int) -> None:
    pass
