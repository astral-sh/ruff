from typing import Protocol

class A(Protocol):
    @property
    def f(self): ...

type Recursive = int | tuple[Recursive, ...]

class B[T: A]: ...

class C[T: A](A):
    x: tuple[Recursive, ...]

class D(B[C]): ...
