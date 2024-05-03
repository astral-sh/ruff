from typing import Container, Generic, Iterable, Sized, Tuple, TypeVar
import typing as t

T = TypeVar('T')
K = TypeVar('K')
V = TypeVar('V')

class LinkedList(Generic[T], Sized):  # PYI059
    def __init__(self) -> None: ...
    def push(self, item: T) -> None: ...

class MyMapping(  # PYI059
    t.Generic[K, V],
    Iterable[Tuple[K, V]],
    Container[Tuple[K, V]],
):
    ...

# Inheriting from just `Generic` is a TypeError, but it's probably fine
# to flag this issue in this case as well, since after fixing the error
# the Generic's position issue persists.
class Foo(Generic, LinkedList): ...  # PYI059

# in case of multiple Generic[] inheritance, don't fix it.
class C(Generic[T], Generic[K, V]): ...  # PYI059


# Negative cases
class MyList(Sized, Generic[T]):  # Generic already in last place
    def __init__(self) -> None: ...

class SomeGeneric(Generic[T]):  # Only one generic
    ...
