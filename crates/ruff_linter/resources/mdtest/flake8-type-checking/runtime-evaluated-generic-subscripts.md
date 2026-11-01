# Runtime required/ambiguous generic subscript handling

```toml
target-version = "py314"

[lint]
select = ["TC001", "TC002", "TC003", "TC004"]

[lint.flake8-type-checking.runtime-evaluated-generic-subscripts]
"injector.Inject" = "required"
"sqlalchemy.orm.Mapped" = "ambiguous"
```

## SQLAlchemy

SQLAlchemy has some very specific runtime requirements for annotations
on model classes. Any annotation wrapped in `sqlalchemy.orm.Mapped`
needs to be available at runtime, unless it is another model class
that shares the same metadata object. Since we don't know which symbols
are excluded from runtime requirements, it's best to treat them as
runtime ambiguous and assume the imports already are in the right place

```py
from sqlalchemy.orm import BaseModel, relationship
from typing import TYPE_CHECKING, ClassVar
from uuid import UUID  # no diagnostic

if TYPE_CHECKING:
    from lib import Default  # no diagnostic
    from sqlalchemy.orm import Mapped  # snapshot: runtime-import-in-type-checking-block
    from .bar import Bar  # no diagnostic

class Foo(BaseModel):
    id: Mapped[UUID]
    bar: Mapped[Bar] = relationship()
    baz: ClassVar[Default]
```

```snapshot
error[TC004]: Move import `sqlalchemy.orm.Mapped` out of type-checking block. Import is used for more than type hinting.
  --> src/mdtest_snippet.py:7:32
   |
 7 |     from sqlalchemy.orm import Mapped  # snapshot: runtime-import-in-type-checking-block
   |                                ^^^^^^
   |
  ::: src/mdtest_snippet.py:11:9
   |
11 |     id: Mapped[UUID]
   |         ------ Used at runtime here
help: Move out of type-checking block
  |
3 | from uuid import UUID  # no diagnostic
4 + from sqlalchemy.orm import Mapped
5 |
6 | if TYPE_CHECKING:
7 |     from lib import Default  # no diagnostic
  -     from sqlalchemy.orm import Mapped  # snapshot: runtime-import-in-type-checking-block
8 |     from .bar import Bar  # no diagnostic
  |
note: This is an unsafe fix and may change runtime behavior
```

## Injector

Injector uses `Inject` to mark parameters which can be injected and
signals intent that this function will be handed to an injector, which
needs runtime access to all annotations.

While it's possible to use the `@inject` decorator instead and add it
to `runtime-evaluated-decorators`, we can also support this other
spelling. It is however less strict, since all the unmarked parameters
will only be runtime ambiguous

```py
from injector import Inject  # no diagnostic
from third_party import Ambiguous1  # no diagnostic

if TYPE_CHECKING:
    from third_party import Ambiguous2  # no diagnostic
    from .first_party import Required  # snapshot: runtime-import-in-type-checking-block

def fun(x: Inject[Required], y: Ambiguous1) -> Ambiguous2:
  pass
```

```snapshot
error[TC004]: Move import `.first_party.Required` out of type-checking block. Import is used for more than type hinting.
 --> src/mdtest_snippet.py:6:30
  |
6 |     from .first_party import Required  # snapshot: runtime-import-in-type-checking-block
  |                              ^^^^^^^^
7 |
8 | def fun(x: Inject[Required], y: Ambiguous1) -> Ambiguous2:
  |                   -------- Used at runtime here
help: Move out of type-checking block
  |
2 | from third_party import Ambiguous1  # no diagnostic
3 + from .first_party import Required
4 |
5 | if TYPE_CHECKING:
6 |     from third_party import Ambiguous2  # no diagnostic
  -     from .first_party import Required  # snapshot: runtime-import-in-type-checking-block
7 |
  |
note: This is an unsafe fix and may change runtime behavior
```
