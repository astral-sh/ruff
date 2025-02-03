# bound
class Foo[_T: str]:
    var: _T


# constraint
class Foo[_T: (str, bytes)]:
    var: _T


# tuple
class Foo[*_Ts]:
    var: tuple[*_Ts]


# paramspec
class C[**_P]:
    var: _P
