# Tests for the `@typing(_extensions).final` decorator

## Cannot subclass

Don't do this:

```py
import typing_extensions
from typing import final

@final
class A: ...

class B(A): ...  # error: 9 [subclass-of-final-class] "Class `B` cannot inherit from final class `A`"

@typing_extensions.final
class C: ...

class D(C): ...  # error: [subclass-of-final-class]
class E: ...
class F: ...
class G: ...

# fmt: off
class H(
    E,
    F,
    A,  # error: [subclass-of-final-class]
    G,
): ...
```
