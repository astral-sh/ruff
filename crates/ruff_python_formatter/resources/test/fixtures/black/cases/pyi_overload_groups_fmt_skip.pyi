from typing import overload

@overload
def f(x: int) -> int: ...
@overload
def f(x: str) -> str: ...  # fmt: skip
def f(x): ...

@overload
async def g(x: int) -> int: ...
@overload
async def g(x: str) -> str: ...  # fmt: skip

class C:
    @overload
    def m(self, x: int) -> int: ...
    @overload
    def m(self, x: str) -> str: ...  # fmt: skip
    @property
    def x(self) -> int: ...
    @x.setter
    def x(self, v: int) -> None: ...  # fmt: skip
