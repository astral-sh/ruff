# `function-call-in-default-argument` (`B008`)

```toml
lint.select = ["B008"]
```

## Frozen dataclasses

Like `RUF009`, `B008` allows frozen dataclass instances as defaults even when their fields are mutable.

```py
from dataclasses import dataclass, field

@dataclass(frozen=True)
class Frozen:
    items: list[int] = field(default_factory=list)

def f(value=Frozen()): ...
```

A non-frozen dataclass still triggers `B008`.

```py
@dataclass
class Mutable: ...

def f(value=Mutable()): ...  # error: [function-call-in-default-argument]
```

Calls inside the constructor arguments are still checked.

```py
def get_items() -> list[int]:
    return []

def f(value=Frozen(get_items())): ...  # error: [function-call-in-default-argument] "Do not perform function call `get_items`"
```

## Name resolution in defaults

A method default can refer to a frozen dataclass in the enclosing class body, even when the parameter
has the same name.

```py
from dataclasses import dataclass

class Client:
    @dataclass(frozen=True)
    class Config: ...

    def connect(self, Config=Config()): ...
```
