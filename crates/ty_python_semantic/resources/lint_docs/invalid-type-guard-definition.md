## What it does
Checks for type guard functions without
a first non-self-like non-keyword-only non-variadic parameter.

## Why is this bad?
Type narrowing functions must accept at least one positional argument
(non-static methods must accept another in addition to `self`/`cls`).

Extra parameters/arguments are allowed but do not affect narrowing.

## Examples
```python
from typing import TypeIs

def f() -> TypeIs[int]: ...  # Error, no parameter
def f(*, v: object) -> TypeIs[int]: ...  # Error, no positional arguments allowed
def f(*args: object) -> TypeIs[int]: ... # Error, expect variadic arguments
class C:
    def f(self) -> TypeIs[int]: ...  # Error, only positional argument expected is `self`
```
