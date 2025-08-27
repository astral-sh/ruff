# Identical display names error messages

<!-- snapshot-diagnostics -->

ty prints the fully qualified name to disambiguate objects with the same name.

## Nested class with identical names

```py
class A:
    class B:
        pass

class C:
    class B:
        pass

a: A.B = C.B()  # error: [invalid-assignment] "Object of type `mdtest_snippet.C.B` is not assignable to `mdtest_snippet.A.B`"
```

## Class from different modules with identical names

```py
import a
import b

df: a.DataFrame = b.DataFrame()  # error: [invalid-assignment] "Object of type `b.DataFrame` is not assignable to `a.DataFrame`"

def _(dfs: list[b.DataFrame]):
    # TODO should be"Object of type `list[b.DataFrame]` is not assignable to `list[a.DataFrame]`
    # error: [invalid-assignment] "Object of type `list[DataFrame]` is not assignable to `list[DataFrame]`"
    dataframes: list[a.DataFrame] = dfs
```

`a.py`:

```py
class DataFrame:
    pass
```

`b.py`:

```py
class DataFrame:
    pass
```

## Enum from different modules with identical names

```py
import status_a
import status_b

# error: [invalid-assignment] "Object of type `Literal[status_b.Status.ACTIVE]` is not assignable to `status_a.Status`"
s: status_a.Status = status_b.Status.ACTIVE
```

`status_a.py`:

```py
from enum import Enum

class Status(Enum):
    ACTIVE = 1
    INACTIVE = 2
```

`status_b.py`:

```py
from enum import Enum

class Status(Enum):
    ACTIVE = "active"
    INACTIVE = "inactive"
```

## Nested enum with identical names

```py
from enum import Enum

class A:
    class B(Enum):
        ACTIVE = "active"
        INACTIVE = "inactive"

class C:
    class B(Enum):
        ACTIVE = "active"
        INACTIVE = "inactive"

# error: [invalid-assignment] "Object of type `Literal[mdtest_snippet.C.B.ACTIVE]` is not assignable to `mdtest_snippet.A.B`"
a: A.B = C.B.ACTIVE
```
