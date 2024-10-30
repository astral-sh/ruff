from typing import Generic, TypeVarTuple, Unpack

Shape = TypeVarTuple('Shape')

class C(Generic[Unpack[Shape]]):
    pass

class D(Generic[Unpack  [Shape]]):
    pass

def f(*args: Unpack[tuple[int, ...]]): pass

def foo(*args: Unpack[int | str]) -> None: pass  # not supported
