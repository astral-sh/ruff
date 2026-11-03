## What it does

Reports `TypedDict` constructions where an unknown key is or may be provided.

## Why is this bad?

An unexpected key in a constructor often means a field name was misspelled. Even an implicitly open
`TypedDict` rejects an explicitly supplied key it does not declare.

## Examples

```python
from typing_extensions import TypedDict, NotRequired


class Person(TypedDict):
    name: str
    age: NotRequired[int]


# error: "Unknown key "aeg""
carol = Person(name="Carol", aeg=25)  # typo!


# error: "Unknown key "nickname""
bob: Person = {"name": "Robert", "nickname": "Bob", "age": 30}  # unknown key!
```

If a `TypedDict` is declared with `closed=True`, this rule will also flag a string key being passed
to the constructor, as this might introduce an illegal extra item:

```py
import json
from typing_extensions import TypedDict


class NotificationPatch(TypedDict, closed=True, total=False):
    email: bool
    sms: bool


def encode_patch(channel: str, enabled: bool) -> str:
    # error: "Non-literal string key may be unknown"
    payload: NotificationPatch = {channel: enabled}
    return json.dumps(payload)
```
