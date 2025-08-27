from typing import Protocol

class Foo[T]: ...

class A(Protocol):
    @property
    def _(self: "A") -> Foo: ...

class B(Protocol):
    @property
    def b(self) -> Foo[A]: ...

class C(Undefined): ...

class D:
    b: Foo[C]

class E[T: B](Protocol): ...

x: E[D]