# Implicit re-export

## Disabled by default

```toml
[rules]
implicit-reexport = "ignore"
```

```py
from a import Answer

reveal_type(Answer)  # revealed: Literal[42]
```

`a.py`:

```py
from b import Answer
```

`b.py`:

```py
Answer = 42
```

## Implicit re-export from a runtime module

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `a` does not explicitly export attribute `Answer`"
from a import Answer

reveal_type(Answer)  # revealed: Literal[42]
```

`a.py`:

```py
from b import Answer
```

`b.py`:

```py
Answer = 42
```

## Aliased import

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `a` does not explicitly export attribute `Answer`"
from a import Answer as Renamed

reveal_type(Renamed)  # revealed: Literal[42]
```

`a.py`:

```py
from b import Answer
```

`b.py`:

```py
Answer = 42
```

## Renamed source import

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `a` does not explicitly export attribute `PublicAnswer`"
from a import PublicAnswer

reveal_type(PublicAnswer)  # revealed: Literal[42]
```

`a.py`:

```py
from b import Answer as PublicAnswer
```

`b.py`:

```py
Answer = 42
```

## Explicit re-exports

```toml
[rules]
implicit-reexport = "error"
```

```py
from a import ExplicitAlias, ExportedInAll, helper

reveal_type(ExplicitAlias)  # revealed: Literal[1]
reveal_type(ExportedInAll)  # revealed: Literal[2]
reveal_type(helper)  # revealed: <module 'helper'>
```

`a.py`:

```py
from b import ExplicitAlias as ExplicitAlias
from b import ExportedInAll
import helper as helper

__all__ = ["ExportedInAll"]
```

`b.py`:

```py
ExplicitAlias = 1
ExportedInAll = 2
```

`helper.py`:

```py
```

## Direct definition

```toml
[rules]
implicit-reexport = "error"
```

```py
from a import Answer

reveal_type(Answer)  # revealed: Literal[42]
```

`a.py`:

```py
Answer = 42
```

## Stub files require explicit re-exports unconditionally

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [unresolved-import] "Module `a` has no member `Answer`"
from a import Answer

reveal_type(Answer)  # revealed: Unknown
```

`a.pyi`:

```pyi
from b import Answer
```

`b.pyi`:

```pyi
Answer: int
```

## Star import with one implicit re-export

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `a` does not explicitly export attribute `Answer`"
from a import *

reveal_type(Answer)  # revealed: Literal[42]
```

`a.py`:

```py
from b import Answer
```

`b.py`:

```py
Answer = 42
```

## Star import with more than two implicit re-exports

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Wildcard import from module `a` includes attributes `Answer`, `Fifth`, and 3 more that are not explicitly exported"
from a import *

reveal_type(Answer)  # revealed: Literal[42]
reveal_type(Other)  # revealed: Literal[43]
reveal_type(Third)  # revealed: Literal[44]
reveal_type(Fourth)  # revealed: Literal[45]
reveal_type(Fifth)  # revealed: Literal[46]
```

`a.py`:

```py
from b import Answer, Other, Third, Fourth, Fifth
```

`b.py`:

```py
Answer = 42
Other = 43
Third = 44
Fourth = 45
Fifth = 46
```

## Star import with two implicit re-exports

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Wildcard import from module `a` includes attributes `Answer` and `Other` that are not explicitly exported"
from a import *

reveal_type(Answer)  # revealed: Literal[42]
reveal_type(Other)  # revealed: Literal[43]
```

`a.py`:

```py
from b import Answer, Other
```

`b.py`:

```py
Answer = 42
Other = 43
```

## Attribute access

```toml
[rules]
implicit-reexport = "error"
```

```py
import a

# error: [implicit-reexport] "Module `a` does not explicitly export attribute `Answer`"
reveal_type(a.Answer)  # revealed: Literal[42]

reveal_type(a.Explicit)  # revealed: Literal[43]
reveal_type(a.ExportedInAll)  # revealed: Literal[44]
```

`a.py`:

```py
from b import Answer
from b import Explicit as Explicit
from b import ExportedInAll

__all__ = ["ExportedInAll"]
```

`b.py`:

```py
Answer = 42
Explicit = 43
ExportedInAll = 44
```

## Package re-exports

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `package` does not explicitly export attribute `Answer`"
from package import Answer

# error: [implicit-reexport] "Module `package` does not explicitly export attribute `nested`"
from package import nested
from package import child

reveal_type(Answer)  # revealed: Literal[42]
reveal_type(nested)  # revealed: <module 'package.sub.nested'>
reveal_type(child)  # revealed: <module 'package.child'>
```

`package/__init__.py`:

```py
from .child import Answer
from .sub import nested
```

`package/child.py`:

```py
Answer = 42
```

`package/sub/__init__.py`:

```py
```

`package/sub/nested.py`:

```py
```

## Imported submodule alias

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `q` does not explicitly export attribute `b`"
from q import a, b

reveal_type(b)  # revealed: <module 'a.b'>
reveal_type(b.C)  # revealed: <class 'C'>

reveal_type(a.b)  # revealed: <module 'a.b'>
reveal_type(a.b.C)  # revealed: <class 'C'>
```

`a/__init__.py`:

```py
```

`a/b.py`:

```py
class C: ...
```

`q.py`:

```py
import a as a
import a.b as b
```

## Star imports respect `__all__`

```toml
[rules]
implicit-reexport = "error"
```

```py
from a import *

reveal_type(Answer)  # revealed: Literal[42]
```

`a.py`:

```py
from b import Answer, Hidden

__all__ = ["Answer"]
```

`b.py`:

```py
Answer = 42
Hidden = 43
```

## Conditional implicit re-export

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `a` does not explicitly export attribute `Answer`"
from a import Answer

reveal_type(Answer)  # revealed: Literal[1, 2]
```

`a.py`:

```py
from b import One, Two

def coinflip() -> bool:
    return True

if coinflip():
    from b import One as Answer
else:
    from b import Two as Answer
```

`b.py`:

```py
One = 1
Two = 2
```

## Conditional explicit and implicit re-export

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `a` does not explicitly export attribute `Answer`"
from a import Answer

reveal_type(Answer)  # revealed: Literal[42]
```

`a.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    from b import Answer
else:
    from b import Answer as Answer
```

`b.py`:

```py
Answer = 42
```

## Conditional explicit re-export

```toml
[rules]
implicit-reexport = "error"
```

```py
from a import Answer

reveal_type(Answer)  # revealed: Literal[1, 2]
```

`a.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    from b import Answer as Answer
else:
    from c import Answer as Answer
```

`b.py`:

```py
Answer = 1
```

`c.py`:

```py
Answer = 2
```

## Synthetic bindings preserve direct exports

```toml
[rules]
implicit-reexport = "error"
```

```py
from a import Answer
```

`a.py`:

```py
Answer = 42

def replace_answer() -> None:
    global Answer
    Answer = 43
```

## Loop re-export

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `a` does not explicitly export attribute `Answer`"
from a import Answer, Explicit
```

`a.py`:

```py
while True:
    from b import Answer
    from b import Explicit as Explicit

    break
```

`b.py`:

```py
Answer = 42
Explicit = 43
```
