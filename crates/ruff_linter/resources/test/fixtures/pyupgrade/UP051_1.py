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
