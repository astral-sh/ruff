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

# Negative cases
class MyList(Sized, Generic[T]):  # Generic already in last place
    def __init__(self) -> None: ...

class SomeGeneric(Generic[T]):  # Only one generic
    ...
