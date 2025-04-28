# `TypedDict`

We do not support `TypedDict`s yet. This test mainly exists to make sure that we do not emit any
errors for the definition of a `TypedDict`.

```py
from typing_extensions import TypedDict, Required

class Person(TypedDict):
    name: str
    age: int | None

# TODO: This should not be an error:
# error: [invalid-assignment]
alice: Person = {"name": "Alice", "age": 30}

# Alternative syntax
Message = TypedDict("Message", {"id": Required[int], "content": str}, total=False)

msg = Message(id=1, content="Hello")

# No errors for yet-unsupported features (`closed`):
OtherMessage = TypedDict("OtherMessage", {"id": int, "content": str}, closed=True)

reveal_type(Person.__required_keys__)  # revealed: @Todo(TypedDict)
reveal_type(Message.__required_keys__)  # revealed: @Todo(TypedDict)
```
