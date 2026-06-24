# Sentinels

## `typing_extensions.Sentinel`

Sentinels constructed with `typing_extensions.Sentinel` can be used directly in type expressions:

```py
from typing_extensions import Sentinel, assert_type

MISSING = Sentinel("MISSING")
OTHER = Sentinel("OTHER")
WITH_REPR = Sentinel("WITH_REPR", "<with repr>")
WITH_REPR_KEYWORD = Sentinel("WITH_REPR_KEYWORD", repr="<with repr keyword>")

reveal_type(MISSING)  # revealed: MISSING
reveal_type(OTHER)  # revealed: OTHER
reveal_type(WITH_REPR)  # revealed: WITH_REPR
reveal_type(WITH_REPR_KEYWORD)  # revealed: WITH_REPR_KEYWORD

def accepts_missing(x: MISSING) -> None: ...
def accepts_other(x: OTHER) -> None: ...

accepts_missing(MISSING)
accepts_missing(OTHER)  # error: [invalid-argument-type]
accepts_other(OTHER)
accepts_other(MISSING)  # error: [invalid-argument-type]

def bad_default(x: int = MISSING) -> None:  # error: [invalid-parameter-default]
    pass

def good_default(x: int | MISSING | OTHER = MISSING) -> None:
    if x is MISSING:
        assert_type(x, MISSING)
        reveal_type(x)  # revealed: MISSING
    else:
        assert_type(x, int | OTHER)
        reveal_type(x)  # revealed: int | OTHER

good_default(1)
good_default(MISSING)
good_default(OTHER)

def reverse_check(x: int | MISSING | OTHER) -> None:
    if MISSING is x:
        assert_type(x, MISSING)
        reveal_type(x)  # revealed: MISSING
    else:
        assert_type(x, int | OTHER)
        reveal_type(x)  # revealed: int | OTHER

def negative_check(x: int | MISSING | OTHER) -> None:
    if x is not MISSING:
        assert_type(x, int | OTHER)
        reveal_type(x)  # revealed: int | OTHER
    else:
        assert_type(x, MISSING)
        reveal_type(x)  # revealed: MISSING

def reverse_negative_check(x: int | MISSING | OTHER) -> None:
    if MISSING is not x:
        assert_type(x, int | OTHER)
        reveal_type(x)  # revealed: int | OTHER
    else:
        assert_type(x, MISSING)
        reveal_type(x)  # revealed: MISSING
```

Sentinel objects are always truthy, expose the standard sentinel metadata attributes, and are
rejected as class bases:

```py
from typing_extensions import Sentinel

MISSING = Sentinel("MISSING")

reveal_type(bool(MISSING))  # revealed: Literal[True]
reveal_type(MISSING.__module__)  # revealed: str

class MissingSubclass(MISSING):  # error: [invalid-base]
    pass
```

Sentinels declared in class scope can also be used in type expressions:

```py
from typing_extensions import Sentinel, assert_type

class C:
    MARKER = Sentinel("C.MARKER")

def accepts_marker(x: C.MARKER) -> None: ...

accepts_marker(C.MARKER)

def class_default(x: int | C.MARKER = C.MARKER) -> None:
    if x is C.MARKER:
        assert_type(x, C.MARKER)
        reveal_type(x)  # revealed: MARKER
    else:
        assert_type(x, int)
        reveal_type(x)  # revealed: int

def class_reverse_negative(x: int | C.MARKER) -> None:
    if C.MARKER is not x:
        assert_type(x, int)
        reveal_type(x)  # revealed: int
    else:
        assert_type(x, C.MARKER)
        reveal_type(x)  # revealed: MARKER
```

Sentinel declarations are recognized only in module and class scope:

```py
from typing_extensions import Sentinel

def outer():
    LOCAL = Sentinel("LOCAL")

    def inner(x: LOCAL) -> None: ...  # error: [invalid-type-form]
```

Sentinels are not generic:

```py
from typing_extensions import Sentinel

MISSING = Sentinel("MISSING")

def f(x: MISSING[int]) -> None: ...  # error: [invalid-type-form]
```

Invalid sentinel constructor calls fall back to the normal call path:

```py
from typing_extensions import Sentinel

NAME = "NAME"

NON_LITERAL_NAME = Sentinel(NAME)
UNKNOWN_NAME = Sentinel(UNKNOWN)  # error: [unresolved-reference]
NON_LITERAL_REPR = Sentinel("NON_LITERAL_REPR", repr=NAME)
UNKNOWN_REPR = Sentinel("UNKNOWN_REPR", repr=UNKNOWN)  # error: [unresolved-reference]
UNKNOWN_KEYWORD = Sentinel("UNKNOWN_KEYWORD", unknown=NAME)  # error: [unknown-argument]
```
