from typing import Any, Type

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


# OK
def foo(a: int, *args: str, **kwargs: str) -> int:
    pass


# ANN401
def foo(a: Any, *args: str, **kwargs: str) -> int:
    pass


# ANN401
def foo(a: int, *args: str, **kwargs: str) -> Any:
    pass


# ANN401
def foo(a: int, *args: Any, **kwargs: Any) -> int:
    pass


# ANN401
def foo(a: int, *args: Any, **kwargs: str) -> int:
    pass


# ANN401
def foo(a: int, *args: str, **kwargs: Any) -> int:
    pass


class Foo:
    # OK
    def foo(self: "Foo", a: int, b: int) -> int:
        pass

    # ANN101
    def foo(self, a: int, b: int) -> int:
        pass

    # ANN401
    def foo(self: "Foo", a: Any, *params: str, **options: str) -> int:
        pass

    # ANN401
    def foo(self: "Foo", a: int, *params: str, **options: str) -> Any:
        pass

    # ANN401
    def foo(self: "Foo", a: int, *params: Any, **options: Any) -> int:
        pass

    # ANN401
    def foo(self: "Foo", a: int, *params: Any, **options: str) -> int:
        pass

    # ANN401
    def foo(self: "Foo", a: int, *params: str, **options: Any) -> int:
        pass

    # OK
    @classmethod
    def foo(cls: Type["Foo"], a: int, b: int) -> int:
        pass

    # ANN102
    @classmethod
    def foo(cls, a: int, b: int) -> int:
        pass
