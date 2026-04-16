# `non-pep695-generic-class` (`UP046`)

```toml
target-version = "py312"
lint.select = ["UP046"]
```

## Basic tests

```py
from typing import Any, AnyStr, Generic, ParamSpec, TypeVar, TypeVarTuple

from somewhere import SupportsRichComparisonT

S = TypeVar("S", str, bytes)  # constrained type variable
T = TypeVar("T", bound=float)
Ts = TypeVarTuple("Ts")
P = ParamSpec("P")


class A(Generic[T]):  # snapshot: non-pep695-generic-class
    # Comments in a class body are preserved
    var: T


class B(Generic[*Ts]):  # snapshot: non-pep695-generic-class
    var: tuple[*Ts]


class C(Generic[P]):  # snapshot: non-pep695-generic-class
    var: P


class Constrained(Generic[S]):  # snapshot: non-pep695-generic-class
    var: S


# This case gets a diagnostic but not a fix because we can't look up the bounds
# or constraints on the TypeVar imported from another module
class ExternalType(Generic[T, SupportsRichComparisonT]):  # snapshot: non-pep695-generic-class
    var: T
    compare: SupportsRichComparisonT


# typing.AnyStr is a common external type variable, so treat it specially as a
# known TypeVar
class MyStr(Generic[AnyStr]):  # snapshot: non-pep695-generic-class
    s: AnyStr


class MultipleGenerics(Generic[S, T, *Ts, P]):  # snapshot: non-pep695-generic-class
    var: S
    typ: T
    tup: tuple[*Ts]
    pep: P


class MultipleBaseClasses(list, Generic[T]):  # snapshot: non-pep695-generic-class
    var: T


# these are just for the MoreBaseClasses and MultipleBaseAndGenerics cases
class Base1: ...


class Base2: ...


class Base3: ...


class MoreBaseClasses(Base1, Base2, Base3, Generic[T]):  # snapshot: non-pep695-generic-class
    var: T


class MultipleBaseAndGenerics(Base1, Base2, Base3, Generic[S, T, *Ts, P]):  # snapshot: non-pep695-generic-class
    var: S
    typ: T
    tup: tuple[*Ts]
    pep: P


class A(Generic[T]): ...  # snapshot: non-pep695-generic-class


class B(A[S], Generic[S]):  # snapshot: non-pep695-generic-class
    var: S


class C(A[S], Generic[S, T]):  # snapshot: non-pep695-generic-class
    var: tuple[S, T]


class D(A[int], Generic[T]):  # snapshot: non-pep695-generic-class
    var: T


class NotLast(Generic[T], Base1):  # snapshot: non-pep695-generic-class
    var: T


class Sandwich(Base1, Generic[T], Base2):  # snapshot: non-pep695-generic-class
    var: T


# runtime `TypeError` to inherit from `Generic` multiple times, but we still
# emit a diagnostic
class TooManyGenerics(Generic[T], Generic[S]):  # snapshot: non-pep695-generic-class
    var: T
    var: S


# These cases are not handled
class D(Generic[T, T]):  # duplicate generic variable, runtime error
    pass


# TODO(brent) we should also apply the fix to methods, but it will need a
# little more work. these should be left alone for now but be fixed eventually.
class NotGeneric:
    # -> generic_method[T: float](t: T)
    def generic_method(t: T) -> T:
        return t


# This one is strange in particular because of the mix of old- and new-style
# generics, but according to the PEP, this is okay "if the class, function, or
# type alias does not use the new syntax." `more_generic` doesn't use the new
# syntax, so it can use T from the module and U from the class scope.
class MixedGenerics[U]:
    def more_generic(u: U, t: T) -> tuple[U, T]:
        return (u, t)


# default requires 3.13
V = TypeVar("V", default=Any, bound=str)


class DefaultTypeVar(Generic[V]):  # -> [V: str = Any]
    var: V


# Test case for TypeVar with default but no bound
W = TypeVar("W", default=int)


class DefaultOnlyTypeVar(Generic[W]):  # -> [W = int]
    var: W


# nested classes and functions are skipped
class Outer:
    class Inner(Generic[T]):
        var: T
```

```snapshot
error[UP046]: Generic class `A` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:11:9
   |
11 | class A(Generic[T]):  # snapshot: non-pep695-generic-class
   |         ^^^^^^^^^^
   |
help: Use type parameters
8  | P = ParamSpec("P")
9  |
10 |
   - class A(Generic[T]):  # snapshot: non-pep695-generic-class
11 + class A[T: float]:  # snapshot: non-pep695-generic-class
12 |     # Comments in a class body are preserved
13 |     var: T
14 |
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `B` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:16:9
   |
16 | class B(Generic[*Ts]):  # snapshot: non-pep695-generic-class
   |         ^^^^^^^^^^^^
   |
help: Use type parameters
13 |     var: T
14 |
15 |
   - class B(Generic[*Ts]):  # snapshot: non-pep695-generic-class
16 + class B[*Ts]:  # snapshot: non-pep695-generic-class
17 |     var: tuple[*Ts]
18 |
19 |
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `C` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:20:9
   |
20 | class C(Generic[P]):  # snapshot: non-pep695-generic-class
   |         ^^^^^^^^^^
   |
help: Use type parameters
17 |     var: tuple[*Ts]
18 |
19 |
   - class C(Generic[P]):  # snapshot: non-pep695-generic-class
20 + class C[**P]:  # snapshot: non-pep695-generic-class
21 |     var: P
22 |
23 |
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `Constrained` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:24:19
   |
24 | class Constrained(Generic[S]):  # snapshot: non-pep695-generic-class
   |                   ^^^^^^^^^^
   |
help: Use type parameters
21 |     var: P
22 |
23 |
   - class Constrained(Generic[S]):  # snapshot: non-pep695-generic-class
24 + class Constrained[S: (str, bytes)]:  # snapshot: non-pep695-generic-class
25 |     var: S
26 |
27 |
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `ExternalType` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:30:20
   |
30 | class ExternalType(Generic[T, SupportsRichComparisonT]):  # snapshot: non-pep695-generic-class
   |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
help: Use type parameters


error[UP046]: Generic class `MyStr` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:37:13
   |
37 | class MyStr(Generic[AnyStr]):  # snapshot: non-pep695-generic-class
   |             ^^^^^^^^^^^^^^^
   |
help: Use type parameters
34 |
35 | # typing.AnyStr is a common external type variable, so treat it specially as a
36 | # known TypeVar
   - class MyStr(Generic[AnyStr]):  # snapshot: non-pep695-generic-class
37 + class MyStr[AnyStr: (bytes, str)]:  # snapshot: non-pep695-generic-class
38 |     s: AnyStr
39 |
40 |
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `MultipleGenerics` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:41:24
   |
41 | class MultipleGenerics(Generic[S, T, *Ts, P]):  # snapshot: non-pep695-generic-class
   |                        ^^^^^^^^^^^^^^^^^^^^^
   |
help: Use type parameters
38 |     s: AnyStr
39 |
40 |
   - class MultipleGenerics(Generic[S, T, *Ts, P]):  # snapshot: non-pep695-generic-class
41 + class MultipleGenerics[S: (str, bytes), T: float, *Ts, **P]:  # snapshot: non-pep695-generic-class
42 |     var: S
43 |     typ: T
44 |     tup: tuple[*Ts]
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `MultipleBaseClasses` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:48:33
   |
48 | class MultipleBaseClasses(list, Generic[T]):  # snapshot: non-pep695-generic-class
   |                                 ^^^^^^^^^^
   |
help: Use type parameters
45 |     pep: P
46 |
47 |
   - class MultipleBaseClasses(list, Generic[T]):  # snapshot: non-pep695-generic-class
48 + class MultipleBaseClasses[T: float](list):  # snapshot: non-pep695-generic-class
49 |     var: T
50 |
51 |
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `MoreBaseClasses` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:62:44
   |
62 | class MoreBaseClasses(Base1, Base2, Base3, Generic[T]):  # snapshot: non-pep695-generic-class
   |                                            ^^^^^^^^^^
   |
help: Use type parameters
59 | class Base3: ...
60 |
61 |
   - class MoreBaseClasses(Base1, Base2, Base3, Generic[T]):  # snapshot: non-pep695-generic-class
62 + class MoreBaseClasses[T: float](Base1, Base2, Base3):  # snapshot: non-pep695-generic-class
63 |     var: T
64 |
65 |
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `MultipleBaseAndGenerics` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:66:52
   |
66 | class MultipleBaseAndGenerics(Base1, Base2, Base3, Generic[S, T, *Ts, P]):  # snapshot: non-pep695-generic-class
   |                                                    ^^^^^^^^^^^^^^^^^^^^^
   |
help: Use type parameters
63 |     var: T
64 |
65 |
   - class MultipleBaseAndGenerics(Base1, Base2, Base3, Generic[S, T, *Ts, P]):  # snapshot: non-pep695-generic-class
66 + class MultipleBaseAndGenerics[S: (str, bytes), T: float, *Ts, **P](Base1, Base2, Base3):  # snapshot: non-pep695-generic-class
67 |     var: S
68 |     typ: T
69 |     tup: tuple[*Ts]
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `A` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:73:9
   |
73 | class A(Generic[T]): ...  # snapshot: non-pep695-generic-class
   |         ^^^^^^^^^^
   |
help: Use type parameters
70 |     pep: P
71 |
72 |
   - class A(Generic[T]): ...  # snapshot: non-pep695-generic-class
73 + class A[T: float]: ...  # snapshot: non-pep695-generic-class
74 |
75 |
76 | class B(A[S], Generic[S]):  # snapshot: non-pep695-generic-class
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `B` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:76:15
   |
76 | class B(A[S], Generic[S]):  # snapshot: non-pep695-generic-class
   |               ^^^^^^^^^^
   |
help: Use type parameters
73 | class A(Generic[T]): ...  # snapshot: non-pep695-generic-class
74 |
75 |
   - class B(A[S], Generic[S]):  # snapshot: non-pep695-generic-class
76 + class B[S: (str, bytes)](A[S]):  # snapshot: non-pep695-generic-class
77 |     var: S
78 |
79 |
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `C` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:80:15
   |
80 | class C(A[S], Generic[S, T]):  # snapshot: non-pep695-generic-class
   |               ^^^^^^^^^^^^^
   |
help: Use type parameters
77 |     var: S
78 |
79 |
   - class C(A[S], Generic[S, T]):  # snapshot: non-pep695-generic-class
80 + class C[S: (str, bytes), T: float](A[S]):  # snapshot: non-pep695-generic-class
81 |     var: tuple[S, T]
82 |
83 |
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `D` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:84:17
   |
84 | class D(A[int], Generic[T]):  # snapshot: non-pep695-generic-class
   |                 ^^^^^^^^^^
   |
help: Use type parameters
81 |     var: tuple[S, T]
82 |
83 |
   - class D(A[int], Generic[T]):  # snapshot: non-pep695-generic-class
84 + class D[T: float](A[int]):  # snapshot: non-pep695-generic-class
85 |     var: T
86 |
87 |
note: This is an unsafe fix and may change runtime behavior


error[UP046]: Generic class `NotLast` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:88:15
   |
88 | class NotLast(Generic[T], Base1):  # snapshot: non-pep695-generic-class
   |               ^^^^^^^^^^
   |
help: Use type parameters


error[UP046]: Generic class `Sandwich` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:92:23
   |
92 | class Sandwich(Base1, Generic[T], Base2):  # snapshot: non-pep695-generic-class
   |                       ^^^^^^^^^^
   |
help: Use type parameters


error[UP046]: Generic class `TooManyGenerics` uses `Generic` subclass instead of type parameters
  --> mdtest_snippet.py:98:23
   |
98 | class TooManyGenerics(Generic[T], Generic[S]):  # snapshot: non-pep695-generic-class
   |                       ^^^^^^^^^^
   |
help: Use type parameters
```

## `AnyStr`

Replacing AnyStr requires specifying the constraints `bytes` and `str`, so
it can't be replaced if these have been shadowed. This test is in a separate
fixture because it doesn't seem possible to restore `str` to its builtin state

```py
from typing import AnyStr, Generic

str = "string"


class BadStr(Generic[AnyStr]):  # snapshot: non-pep695-generic-class
    var: AnyStr
```

```snapshot
error[UP046]: Generic class `BadStr` uses `Generic` subclass instead of type parameters
 --> mdtest_snippet.py:6:14
  |
6 | class BadStr(Generic[AnyStr]):  # snapshot: non-pep695-generic-class
  |              ^^^^^^^^^^^^^^^
  |
help: Use type parameters
```

## `typing_extensions`

This is placed in a separate fixture as `TypeVar` needs to be imported
from `typing_extensions` to support default arguments in Python version < 3.13.
We verify that UP046 doesn't apply in this case.

```py
from typing import Generic
from typing_extensions import TypeVar

T = TypeVar("T", default=str)


class DefaultTypeVar(Generic[T]):
    var: T


class KeywordArguments(Generic[T], metaclass=type):
    var: T
```
