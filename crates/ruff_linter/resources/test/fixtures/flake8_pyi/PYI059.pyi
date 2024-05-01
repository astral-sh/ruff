from typing import Container, Generic, Iterable, Sized, Tuple, TypeVar

T = TypeVar('T')
K = TypeVar('K')
V = TypeVar('V')

class LinkedList(Generic[T], Sized):  # PYI059
    def __init__(self) -> None: ...
    def push(self, item: T) -> None: ...

class MyMapping(  # PYI059
    Generic[K, V],
    Iterable[Tuple[K, V]],
    Container[Tuple[K, V]],
):
    ...
