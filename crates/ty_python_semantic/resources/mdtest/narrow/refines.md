# Receiver replacement with `Refines`

`Refines[After, Returned]` gives the receiver type `After` when the method returns normally. The
call has type `Returned`.

```toml
[environment]
python-version = "3.14"
```

## Adding an attribute

`Self & HasName` preserves the receiver's original type and makes the newly added `name` attribute
available to the caller.

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

## Ordinary calls preserve earlier refinements

A read after conditional writes preserves each possible cell type.

```py
from ty_extensions.experimental import Refines

class Cell[T]:
    def get(self) -> T:
        return self.__dict__["value"]

    def set[U](self, value: U) -> Refines[Cell[U], None]:
        self.__dict__["value"] = value

def update(cell: Cell[int], flag: bool, text: str):
    if flag:
        cell.set(text)
    cell.get()
    reveal_type(cell.get())  # revealed: int | str

def both_branches(cell: Cell[int], flag: bool, text: str, data: bytes):
    if flag:
        cell.set(text)
    else:
        cell.set(data)
    cell.get()
    reveal_type(cell.get())  # revealed: str | bytes
```
