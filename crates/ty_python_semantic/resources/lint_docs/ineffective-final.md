## What it does

Checks for calls to `final()` that type checkers cannot interpret.

## Why is this bad?

The `final()` function is designed to be used as a decorator. When called directly
as a function (e.g., `final(type(...))`), type checkers will not understand the
application of `final` and will not prevent subclassing.

## Example

```python
from typing import final

# Incorrect: type checkers will not prevent subclassing
MyClass = final(type("MyClass", (), {}))  # error


# Correct: use `final` as a decorator
@final
class MyClass: ...
```
