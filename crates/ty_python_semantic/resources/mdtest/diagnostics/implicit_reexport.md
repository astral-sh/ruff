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
# error: [implicit-reexport] "Wildcard import from module `api` includes names that are not explicitly exported"
from api import *
from explicit_api import *

reveal_type(Answer)  # revealed: Literal[42]
reveal_type(Other)  # revealed: Literal[43]
reveal_type(Exported)  # revealed: Literal[44]
```

`api.py`:

```py
from values import Answer, Other
```

`explicit_api.py`:

```py
from values import Exported, Hidden

__all__ = ["Exported"]
```

`values.py`:

```py
Answer = 42
Other = 43
Exported = 44
Hidden = 45
```

## Control flow

```toml
[rules]
implicit-reexport = "error"
```

```py
# error: [implicit-reexport] "Module `flow` does not explicitly export attribute `Mixed`"
from flow import Mixed
from flow import Explicit

reveal_type(Mixed)  # revealed: Literal[42]
reveal_type(Explicit)  # revealed: Literal[1, 2]
```

`flow.py`:

```py
def coinflip() -> bool:
    return True

if coinflip():
    from left import Mixed
else:
    from left import Mixed as Mixed

if coinflip():
    from left import Explicit as Explicit
else:
    from right import Explicit as Explicit
```

`left.py`:

```py
Explicit = 1
Mixed = 42
```

`right.py`:

```py
Explicit = 2
```
