# Singleton types

A type is a singleton type iff it has exactly one inhabitant.

## Basic

```py
from typing_extensions import Literal, Never
from knot_extensions import is_singleton, static_assert

static_assert(is_singleton(None))
static_assert(is_singleton(Literal[True]))
static_assert(is_singleton(Literal[False]))

static_assert(is_singleton(type[bool]))

static_assert(not is_singleton(Never))
static_assert(not is_singleton(str))

static_assert(not is_singleton(Literal[345]))
static_assert(not is_singleton(Literal[1, 2]))

static_assert(not is_singleton(tuple[()]))
static_assert(not is_singleton(tuple[None]))
static_assert(not is_singleton(tuple[None, Literal[True]]))
```

## `NoDefault`

### 3.12

```toml
[environment]
python-version = "3.12"
```

```py
from typing_extensions import _NoDefaultType
from knot_extensions import is_singleton, static_assert

static_assert(is_singleton(_NoDefaultType))
```

### 3.13

```toml
[environment]
python-version = "3.13"
```

```py
from typing import _NoDefaultType
from knot_extensions import is_singleton, static_assert

static_assert(is_singleton(_NoDefaultType))
```
