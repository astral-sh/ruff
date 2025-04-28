# `TypedDict`

We do not support `TypedDict`s yet. This test mainly exists to make sure that we do not emit any
errors for the definition of a `TypedDict`.

```py
from typing_extensions import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

# TODO: This should not be an error:
# error: [invalid-assignment]
p1: Person = {"name": "Alice", "age": 30}

# Alternative syntax
Message = TypedDict("Message", {"id": int, "content": str})

p2 = Message(name="Bob", age=25)
```
