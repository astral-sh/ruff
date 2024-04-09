@function_decorator
def test():
    pass


@class_decorator
class Test:
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


@x := 1
@x if True else y
@lambda x: x
@x and y
@(yield x)
@(*x, *y)
def f(): ...


# This is not multiple decorators on the same line but rather a binary (`@`) expression
@x @y
def foo(): ...


@x


@y


def foo(): ...