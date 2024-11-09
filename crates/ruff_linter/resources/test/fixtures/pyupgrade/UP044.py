from typing import Generic, TypeVarTuple, Unpack

Shape = TypeVarTuple('Shape')

class C(Generic[Unpack[Shape]]):
    pass

class D(Generic[Unpack  [Shape]]):
    pass

def f(*args: Unpack[tuple[int, ...]]): pass

def f(*args: Unpack[other.Type]): pass


# Not valid unpackings but they are valid syntax
def foo(*args: Unpack[int | str]) -> None: pass
def foo(*args: Unpack[int and str]) -> None: pass
def foo(*args: Unpack[int > str]) -> None: pass

# We do not use the shorthand unpacking syntax in the following cases
from typing import TypedDict
class KwargsDict(TypedDict):
    x: int
    y: int

def foo(name: str, /, **kwargs: Unpack[KwargsDict]) -> None: pass
