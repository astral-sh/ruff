# Receiver replacement with `Refines`

`Refines[After, Returned]` gives the receiver type `After` when the method returns normally. The
call has type `Returned`. Here `Self & HasName` preserves the receiver's original type and makes the
newly added `name` attribute available to the caller.

```toml
[environment]
python-version = "3.14"
```

```py
from typing import Protocol, Self, reveal_type
from ty_extensions.experimental import Refines

class HasName(Protocol):
    name: str

class Record:
    def set_name(self, name: str) -> Refines[Self & HasName, int]:
        self.__dict__["name"] = name
        return len(name)

def update(record: Record, flag: bool):
    if flag:
        length = record.set_name("Ada")
        reveal_type(length)  # revealed: int
        reveal_type(record)  # revealed: Record & HasName
        reveal_type(record.name)  # revealed: str
    reveal_type(record)  # revealed: Record
```
