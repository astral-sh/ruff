from __future__ import annotations

from typing import TypeAlias, TYPE_CHECKING

from foo import Foo

if TYPE_CHECKING:
    from typing import Dict

    OptStr: TypeAlias = str | None
    Bar: TypeAlias = Foo[int]
else:
    Bar = Foo

a: TypeAlias = 'int'  # TC008
b: TypeAlias = 'Dict'  # OK
c: TypeAlias = 'Foo'   # TC008
d: TypeAlias = 'Foo[str]'  # OK
e: TypeAlias = 'Foo.bar'  # OK
f: TypeAlias = 'Foo | None'  # TC008
g: TypeAlias = 'OptStr'   # OK
h: TypeAlias = 'Bar'   # TC008
i: TypeAlias = Foo['str']   # TC008
j: TypeAlias = 'Baz'   # OK
k: TypeAlias = 'k | None'  # OK
l: TypeAlias = 'int' | None  # TC008 (because TC010 is not enabled)
m: TypeAlias = ('int'  # TC008
    | None)
n: TypeAlias = ('int'  # TC008 (fix removes comment currently)
    ' | None')

type B = 'Dict'  # TC008
type D = 'Foo[str]'  # TC008
type E = 'Foo.bar'  # TC008
type G = 'OptStr'  # TC008
type I = Foo['str']  # TC008
type J = 'Baz'  # TC008
type K = 'K | None'  # TC008
type L = 'int' | None  # TC008 (because TC010 is not enabled)
type M = ('int'  # TC008
    | None)
type N = ('int'  # TC008 (fix removes comment currently)
    ' | None')


class Baz:
    a: TypeAlias = 'Baz'  # OK
    type A = 'Baz'  # TC008

    class Nested:
        a: TypeAlias = 'Baz'  # OK
        type A = 'Baz'  # TC008

# O should have parenthesis added
o: TypeAlias = """int
| None"""
type O = """int
| None"""

# P, Q, and R should not have parenthesis added
p: TypeAlias = ("""int
| None""")
type P = ("""int
| None""")

q: TypeAlias = """(int
| None)"""
type Q = """(int
| None)"""

r: TypeAlias = """int | None"""
type R = """int | None"""