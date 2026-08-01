# Unsupported base for dynamic `type()` classes

<!-- snapshot-diagnostics -->

## `@final` class

Classes decorated with `@final` cannot be subclassed:

```py
from typing import final

@final
class FinalClass:
    pass

X = type("X", (FinalClass,), {})  # error: [subclass-of-final-class]
```

## `Generic` base

Dynamic classes created via `type()` cannot inherit from `Generic`:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

X = type("X", (Generic[T],), {})  # error: [invalid-base]
```

## `Protocol` base

Dynamic classes created via `type()` cannot inherit from `Protocol`:

```py
from typing import Protocol

X = type("X", (Protocol,), {})  # error: [unsupported-dynamic-base]
```

## `TypedDict` base

Dynamic classes created via `type()` cannot inherit from `TypedDict` directly. Use
`TypedDict("Name", ...)` instead:

```py
from typing_extensions import TypedDict

X = type("X", (TypedDict,), {})  # error: [invalid-base]
```

## Enum base

Dynamic classes created via `type()` cannot inherit from Enum classes because `EnumMeta` expects
special dict attributes that `type()` doesn't provide:

```py
from enum import Enum

class MyEnum(Enum):
    pass

X = type("X", (MyEnum,), {})  # error: [invalid-base]
```

## Enum with members

Enums with members are final and cannot be subclassed at all:

```py
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2

X = type("X", (Color,), {})  # error: [subclass-of-final-class]
```
