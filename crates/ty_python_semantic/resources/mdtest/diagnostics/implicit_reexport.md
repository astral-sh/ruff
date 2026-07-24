# Implicit re-export

## Disabled by default

```toml
[rules]
implicit-reexport = "ignore"
```

```py
from api import Answer

reveal_type(Answer)  # revealed: Literal[42]
```

`api.py`:

```py
from models import Answer
```

`models.py`:

```py
Answer = 42
```

## Named imports and attribute access

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `api` does not explicitly export attribute `Answer`"
from api import Answer

# error: [implicit-reexport] "Module `api` does not explicitly export attribute `PublicAnswer`"
from api import PublicAnswer as Renamed
from api import Direct, ExplicitAlias, ExportedInAll, helper
import api

reveal_type(Answer)  # revealed: Literal[42]
reveal_type(Renamed)  # revealed: Literal[42]
reveal_type(ExplicitAlias)  # revealed: Literal[1]
reveal_type(ExportedInAll)  # revealed: Literal[2]
reveal_type(Direct)  # revealed: Literal[5]
reveal_type(helper)  # revealed: <module 'helper'>

# error: [implicit-reexport] "Module `api` does not explicitly export attribute `Answer`"
reveal_type(api.Answer)  # revealed: Literal[42]
reveal_type(api.ExplicitAlias)  # revealed: Literal[1]
reveal_type(api.ExportedInAll)  # revealed: Literal[2]
```

`api.py`:

```py
from models import Answer
from models import Answer as PublicAnswer
from models import ExplicitAlias as ExplicitAlias
from models import ExportedInAll
import helper as helper

Direct = 5
__all__ = ["ExportedInAll"]
```

`models.py`:

```py
Answer = 42
ExplicitAlias = 1
ExportedInAll = 2
```

`helper.py`:

```py
```

## Stub files still hide implicit re-exports

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [unresolved-import] "Module `api` has no member `Answer`"
from api import Answer

reveal_type(Answer)  # revealed: Unknown
```

`api.pyi`:

```pyi
from models import Answer
```

`models.pyi`:

```pyi
Answer: int
```

## Wildcard imports

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `one` does not explicitly export attribute `One`"
from one import *

# error: [implicit-reexport] "Wildcard import from module `two` includes attributes `TwoA` and `TwoB` that are not explicitly exported"
from two import *

# error: [implicit-reexport] "Wildcard import from module `many` includes attributes `ManyA`, `ManyB`, and 3 more that are not explicitly exported"
from many import *

reveal_type(One)  # revealed: Literal[1]
reveal_type(TwoA)  # revealed: Literal[2]
reveal_type(TwoB)  # revealed: Literal[3]
reveal_type(ManyA)  # revealed: Literal[4]
reveal_type(ManyE)  # revealed: Literal[8]
```

`one.py`:

```py
from values import One
```

`two.py`:

```py
from values import TwoA, TwoB
```

`many.py`:

```py
from values import ManyA, ManyB, ManyC, ManyD, ManyE
```

`values.py`:

```py
One = 1
TwoA = 2
TwoB = 3
ManyA = 4
ManyB = 5
ManyC = 6
ManyD = 7
ManyE = 8
```

## Package and submodule re-exports

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

# error: [implicit-reexport] "Module `q` does not explicitly export attribute `b`"
from q import a, b

reveal_type(Answer)  # revealed: Literal[42]
reveal_type(nested)  # revealed: <module 'package.sub.nested'>
reveal_type(child)  # revealed: <module 'package.child'>
reveal_type(b.C)  # revealed: <class 'C'>
reveal_type(a.b.C)  # revealed: <class 'C'>
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

## Wildcard imports respect `__all__`

```toml
[rules]
implicit-reexport = "error"
```

```py
from api import *

reveal_type(Answer)  # revealed: Literal[42]
```

`api.py`:

```py
from models import Answer, Hidden

__all__ = ["Answer"]
```

`models.py`:

```py
Answer = 42
Hidden = 43
```

## Control flow

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `flow` does not explicitly export attribute `Conditional`"
from flow import Conditional

# error: [implicit-reexport] "Module `flow` does not explicitly export attribute `Mixed`"
from flow import Mixed
from flow import Explicit, Synthetic

# error: [implicit-reexport] "Module `flow` does not explicitly export attribute `LoopImplicit`"
from flow import LoopExplicit, LoopImplicit

reveal_type(Conditional)  # revealed: Literal[1, 2]
reveal_type(Mixed)  # revealed: Literal[42]
reveal_type(Explicit)  # revealed: Literal[1, 2]
```

`flow.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    from left import Conditional
else:
    from right import Conditional

if coinflip():
    from left import Mixed
else:
    from left import Mixed as Mixed

if coinflip():
    from left import Explicit as Explicit
else:
    from right import Explicit as Explicit

Synthetic = 42

def replace_synthetic() -> None:
    global Synthetic
    Synthetic = 43

while True:
    from left import LoopExplicit as LoopExplicit
    from left import LoopImplicit

    break
```

`left.py`:

```py
Conditional = 1
Explicit = 1
LoopExplicit = 1
LoopImplicit = 2
Mixed = 42
```

`right.py`:

```py
Conditional = 2
Explicit = 2
```
