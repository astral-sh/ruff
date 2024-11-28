from __future__ import annotations

from typing import Dict, TypeAlias, TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Dict

    from foo import Foo

    OptStr: TypeAlias = str | None
    Bar: TypeAlias = Foo[int]

a: TypeAlias = int  # OK
b: TypeAlias = Dict  # OK
c: TypeAlias = Foo   # TC007
d: TypeAlias = Foo | None  # TC007
e: TypeAlias = OptStr   # TC007
f: TypeAlias = Bar   # TC007
g: TypeAlias = Foo | Bar  # TC007 x2
h: TypeAlias = Foo[str]  # TC007
i: TypeAlias = (Foo |  # TC007 x2 (fix removes comment currently)
    Bar)

type C = Foo   # OK
type D = Foo | None  # OK
type E = OptStr   # OK
type F = Bar   # OK
type G = Foo | Bar  # OK
type H = Foo[str]  # OK
type I = (Foo |  # OK 
    Bar)
