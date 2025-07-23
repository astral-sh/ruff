# Exhaustiveness checking

```toml
[environment]
python-version = "3.11"
```

## Checks on literals

```py
from typing import Literal, assert_never

def if_else_exhaustive(x: Literal[0, 1, "a"]):
    if x == 0:
        pass
    elif x == 1:
        pass
    elif x == "a":
        pass
    else:
        no_diagnostic_here

        assert_never(x)

def if_else_non_exhaustive(x: Literal[0, 1, "a"]):
    if x == 0:
        pass
    elif x == "a":
        pass
    else:
        this_should_be_an_error  # error: [unresolved-reference]

        assert_never(x)  # error: [type-assertion-failure]

def match_exhaustive(x: Literal[0, 1, "a"]):
    match x:
        case 0:
            pass
        case 1:
            pass
        case "a":
            pass
        case _:
            # TODO: this should not be an error
            no_diagnostic_here  # error: [unresolved-reference]

            assert_never(x)

def match_non_exhaustive(x: Literal[0, 1, "a"]):
    match x:
        case 0:
            pass
        case "a":
            pass
        case _:
            this_should_be_an_error  # error: [unresolved-reference]

            assert_never(x)  # error: [type-assertion-failure]
```

## `isinstance` checks

```py
from typing import assert_never

class A: ...
class B: ...
class C: ...

def if_else_exhaustive(x: A | B | C):
    if isinstance(x, A):
        pass
    elif isinstance(x, B):
        pass
    elif isinstance(x, C):
        pass
    else:
        no_diagnostic_here

        assert_never(x)

def if_else_non_exhaustive(x: A | B | C):
    if isinstance(x, A):
        pass
    elif isinstance(x, C):
        pass
    else:
        this_should_be_an_error  # error: [unresolved-reference]

        assert_never(x)  # error: [type-assertion-failure]

def match_exhaustive(x: A | B | C):
    match x:
        case A():
            pass
        case B():
            pass
        case C():
            pass
        case _:
            # TODO: this should not be an error
            no_diagnostic_here  # error: [unresolved-reference]

            assert_never(x)

def match_non_exhaustive(x: A | B | C):
    match x:
        case A():
            pass
        case C():
            pass
        case _:
            this_should_be_an_error  # error: [unresolved-reference]

            assert_never(x)  # error: [type-assertion-failure]
```
