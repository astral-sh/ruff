from __future__ import annotations

from typing import TypeAlias, TYPE_CHECKING

from foo import Foo

if TYPE_CHECKING:
    from typing import Dict

    OptStr: TypeAlias = str | None
    Bar: TypeAlias = Foo[int]
else:
    Bar = Foo

a: TypeAlias = 'int'  # TCH008
b: TypeAlias = 'Dict'  # OK
c: TypeAlias = 'Foo'   # TCH008
d: TypeAlias = 'Foo[str]'  # OK
e: TypeAlias = 'Foo.bar'  # OK
f: TypeAlias = 'Foo | None'  # TCH008
g: TypeAlias = 'OptStr'   # OK
h: TypeAlias = 'Bar'   # TCH008
i: TypeAlias = Foo['str']   # TCH008
j: TypeAlias = 'Baz'   # OK
k: TypeAlias = 'k | None'  # OK
l: TypeAlias = 'int' | None  # TCH008 (because TC010 is not enabled)
m: TypeAlias = ('int'  # TCH008
    | None)
n: TypeAlias = ('int'  # TCH008 (fix removes comment currently)
    ' | None')

type B = 'Dict'  # TCH008
type D = 'Foo[str]'  # TCH008
type E = 'Foo.bar'  # TCH008
type G = 'OptStr'  # TCH008
type I = Foo['str']  # TCH008
type J = 'Baz'  # TCH008
type K = 'K | None'  # TCH008
type L = 'int' | None  # TCH008 (because TC010 is not enabled)
type M = ('int'  # TCH008
    | None)
type N = ('int'  # TCH008 (fix removes comment currently)
    ' | None')


class Baz:
    a: TypeAlias = 'Baz'  # OK
    type A = 'Baz'  # TCH008

    class Nested:
        a: TypeAlias = 'Baz'  # OK
        type A = 'Baz'  # TCH008
