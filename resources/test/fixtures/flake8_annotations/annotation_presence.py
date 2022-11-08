from typing import Type

# Error
def foo(a, b):
    pass


# Error
def foo(a: int, b):
    pass


# Error
def foo(a: int, b) -> int:
    pass


# Error
def foo(a: int, b: int):
    pass


# Error
def foo():
    pass


# OK
def foo(a: int, b: int) -> int:
    pass


# OK
def foo() -> int:
    pass


class Foo:
    # OK
    def foo(self: "Foo", a: int, b: int) -> int:
        pass

    # ANN101
    def foo(self, a: int, b: int) -> int:
        pass

    # OK
    @classmethod
    def foo(cls: Type["Foo"], a: int, b: int) -> int:
        pass

    # ANN102
    @classmethod
    def foo(cls, a: int, b: int) -> int:
        pass
