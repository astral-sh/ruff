# bound
class Foo[_T: str]:
    var: _T


# constraint
class Foo[_T: (str, bytes)]:
    var: _T


# python 3.13+ default
class Foo[_T = int]:
    var: _T


# tuple
class Foo[*_Ts]:
    var: tuple[*_Ts]


# paramspec
class C[**_P]:
    var: _P


from typing import Callable


# each of these will get a separate diagnostic, but at least they'll all get
# fixed
class Everything[_T, _U: str, _V: (int, float), *_W, **_X]:
    @staticmethod
    def transform(t: _T, u: _U, v: _V) -> tuple[*_W] | Callable[_X, _T] | None:
        return None


# this should not be fixed because the new name is a keyword, but we still
# offer a diagnostic
class F[_async]: ...


# and this should not be fixed because of the conflict with the outer X, but it
# also gets a diagnostic
def f():
    type X = int

    class ScopeConflict[_X]:
        var: _X
        x: X


# these cases should be skipped entirely
def f[_](x: _) -> _: ...
def g[__](x: __) -> __: ...
def h[_T_](x: _T_) -> _T_: ...
def i[__T__](x: __T__) -> __T__: ...


# https://github.com/astral-sh/ruff/issues/16024

from typing import cast, Literal


class C[_0]: ...


class C[T, _T]: ...
class C[_T, T]: ...


class C[_T]:
    v1 = cast(_T, ...)
    v2 = cast('_T', ...)
    v3 = cast("\u005fT", ...)

    def _(self):
        v1 = cast(_T, ...)
        v2 = cast('_T', ...)
        v3 = cast("\u005fT", ...)


class C[_T]:
    v = cast('Literal[\'foo\'] | _T', ...)


## Name collision
class C[T]:
    def f[_T](self):  # No fix, collides with `T` from outer scope
        v1 = cast(_T, ...)
        v2 = cast('_T', ...)


# Unfixable as the new name collides with a variable visible from one of the inner scopes
class C[_T]:
    T = 42

    v1 = cast(_T, ...)
    v2 = cast('_T', ...)


# Unfixable as the new name collides with a variable visible from one of the inner scopes
class C[_T]:
    def f[T](self):
        v1 = cast(_T, ...)
        v2 = cast('_T', ...)
