def f():
    from . import first_party

    def f(x: first_party.foo): ...


# Type parameter bounds
def g():
    from . import foo

    class C[T: foo.Ty]: ...


def h():
    from . import foo

    def f[T: foo.Ty](x: T): ...


def i():
    from . import foo

    type Alias[T: foo.Ty] = list[T]


# Type parameter defaults
def j():
    from . import foo

    class C[T = foo.Ty]: ...


def k():
    from . import foo

    def f[T = foo.Ty](x: T): ...


def l():
    from . import foo

    type Alias[T = foo.Ty] = list[T]


# non-generic type alias
def m():
    from . import foo

    type Alias = foo.Ty


# unions
from typing import Union


def n():
    from . import foo

    def f(x: Union[foo.Ty, int]): ...
    def g(x: foo.Ty | int): ...


# runtime and typing usage
def o():
    from . import foo

    def f(x: foo.Ty):
        return foo.Ty()
