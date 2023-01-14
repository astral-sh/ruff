class C:
    from typing import overload

    @overload
    def f(self, x: int, y: int) -> None:
        ...

    def f(self, x, y):
        pass
