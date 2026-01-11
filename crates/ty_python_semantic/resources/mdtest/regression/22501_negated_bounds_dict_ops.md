# Dict operations after narrowing should still be valid

This is a regression test for <https://github.com/astral-sh/ruff/pull/22501>.

```py
from typing import Any

class OrderedDict(dict[str, Any]):
    def popitem(self) -> tuple[str, Any]:
        if not self:
            raise KeyError("dictionary is empty")
        key = next(iter(self))
        value = dict.pop(self, key)
        return key, value

def redact(kwargs: dict[str, Any | None]) -> None:
    kwargs = {k: v for k, v in kwargs.items() if v is not None}
    if "durationMS" in kwargs:
        kwargs["durationMS"] = 1
        _ = kwargs["durationMS"]
    if "serviceId" in kwargs:
        kwargs["serviceId"] = "srv"
```
