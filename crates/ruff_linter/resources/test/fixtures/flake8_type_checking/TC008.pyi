from __future__ import annotations

from typing import TypeAlias, TYPE_CHECKING

from foo import Foo

a: TypeAlias = 'int'  # TC008
b: TypeAlias = 'Foo'   # TC008
c: TypeAlias = 'Foo[str]'  # TC008
d: TypeAlias = 'Foo.bar'  # TC008

type B = 'Foo'  # TC008
type C = 'Foo[str]'  # TC008
type D = 'Foo.bar'  # TC008


class Baz:
    a: TypeAlias = 'Baz'  # TC008
    type A = 'Baz'  # TC008

    class Nested:
        a: TypeAlias = 'Baz'  # TC008
        type A = 'Baz'  # TC008
