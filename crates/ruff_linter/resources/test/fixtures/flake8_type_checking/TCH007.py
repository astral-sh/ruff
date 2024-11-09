from __future__ import annotations

from typing import Dict, TypeAlias, TYPE_CHECKING

if TYPE_CHECKING:
    from typing import Dict

    from foo import Foo

    OptStr: TypeAlias = str | None
    Bar: TypeAlias = Foo[int]

a: TypeAlias = int  # OK
b: TypeAlias = Dict  # OK
c: TypeAlias = Foo   # TCH007
d: TypeAlias = Foo | None  # TCH007
e: TypeAlias = OptStr   # TCH007
f: TypeAlias = Bar   # TCH007
g: TypeAlias = Foo | Bar  # TCH007 x2
h: TypeAlias = Foo[str]  # TCH007
i: TypeAlias = (Foo |  # TCH007 x2 (fix removes comment currently)
    Bar)

type C = Foo   # OK
type D = Foo | None  # OK
type E = OptStr   # OK
type F = Bar   # OK
type G = Foo | Bar  # OK
type H = Foo[str]  # OK
type I = (Foo |  # OK 
    Bar)
