from __future__ import annotations

from typing import TypeAlias, TYPE_CHECKING

from foo import Foo

if TYPE_CHECKING:
    from typing import Dict

    OptStr: TypeAlias = str | None
    Bar: TypeAlias = Foo[int]

    a: TypeAlias = 'int'  # TC008
    b: TypeAlias = 'Dict'  # TC008
    c: TypeAlias = 'Foo'   # TC008
    d: TypeAlias = 'Foo[str]'  # TC008
    e: TypeAlias = 'Foo.bar'  # TC008
    f: TypeAlias = 'Foo | None'  # TC008
    g: TypeAlias = 'OptStr'   # TC008
    h: TypeAlias = 'Bar'   # TC008
    i: TypeAlias = Foo['str']   # TC008
    j: TypeAlias = 'Baz'   # OK (this would be treated as use before define)
    k: TypeAlias = 'k | None'  # False negative in type checking block
    l: TypeAlias = 'int' | None  # TC008 (because TC010 is not enabled)
    m: TypeAlias = ('int'  # TC008
        | None)
    n: TypeAlias = ('int'  # TC008 (fix removes comment currently)
        ' | None')


    class Baz: ...
