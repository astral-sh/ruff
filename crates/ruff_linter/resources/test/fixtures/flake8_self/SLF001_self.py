from typing import Self


class Foo:
    def __init__(self, x: int) -> None:
        self._x = x

    def foo(self) -> None:
        this: Self = self
        print(this._x)  # OK (Self annotation)
        print(self._x)  # OK (self)

    @classmethod
    def bar(cls) -> None:
        inst: Self = cls()
        print(inst._x)  # OK (Self annotation)

    def other(self, other: Self) -> None:
        print(other._x)  # OK (Self annotation)


class Bar:
    def __init__(self, y: int) -> None:
        self._y = y

    def baz(self, other: Foo) -> None:
        print(other._x)  # SLF001 (not Self, different class)
