@function_decorator
def test():
    pass


@class_decorator
class Abcd:
    pass


@decorator
def f(): ...


@a.b.c
def f(): ...


@a
@a.b.c
def f(): ...


@a
@1 | 2
@a.b.c
class T: ...


@named_expr := abc
def f(): ...