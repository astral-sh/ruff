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

def if_else_exhaustive_no_assertion(x: Literal[0, 1, "a"]) -> int:
    if x == 0:
        return 0
    elif x == 1:
        return 1
    elif x == "a":
        return 2

def if_else_non_exhaustive(x: Literal[0, 1, "a"]):
    if x == 0:
        pass
    elif x == "a":
        pass
    else:
        this_should_be_an_error  # error: [unresolved-reference]

        # this diagnostic is correct: the inferred type of `x` is `Literal[1]`
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
            no_diagnostic_here

            assert_never(x)

def match_exhaustive_no_assertion(x: Literal[0, 1, "a"]) -> int:
    match x:
        case 0:
            return 0
        case 1:
            return 1
        case "a":
            return 2

def match_non_exhaustive(x: Literal[0, 1, "a"]):
    match x:
        case 0:
            pass
        case "a":
            pass
        case _:
            this_should_be_an_error  # error: [unresolved-reference]

            # this diagnostic is correct: the inferred type of `x` is `Literal[1]`
            assert_never(x)  # error: [type-assertion-failure]
```

## Checks on enum literals

```py
from enum import Enum
from typing import assert_never

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

def if_else_exhaustive(x: Color):
    if x == Color.RED:
        pass
    elif x == Color.GREEN:
        pass
    elif x == Color.BLUE:
        pass
    else:
        no_diagnostic_here

        assert_never(x)

def if_else_exhaustive_no_assertion(x: Color) -> int:
    if x == Color.RED:
        return 1
    elif x == Color.GREEN:
        return 2
    elif x == Color.BLUE:
        return 3

def if_else_non_exhaustive(x: Color):
    if x == Color.RED:
        pass
    elif x == Color.BLUE:
        pass
    else:
        this_should_be_an_error  # error: [unresolved-reference]

        # this diagnostic is correct: inferred type of `x` is `Literal[Color.GREEN]`
        assert_never(x)  # error: [type-assertion-failure]

def match_exhaustive(x: Color):
    match x:
        case Color.RED:
            pass
        case Color.GREEN:
            pass
        case Color.BLUE:
            pass
        case _:
            no_diagnostic_here

            assert_never(x)

def match_exhaustive_2(x: Color):
    match x:
        case Color.RED:
            pass
        case Color.GREEN | Color.BLUE:
            pass
        case _:
            no_diagnostic_here

            assert_never(x)

def match_exhaustive_no_assertion(x: Color) -> int:
    match x:
        case Color.RED:
            return 1
        case Color.GREEN:
            return 2
        case Color.BLUE:
            return 3

def match_non_exhaustive(x: Color):
    match x:
        case Color.RED:
            pass
        case Color.BLUE:
            pass
        case _:
            this_should_be_an_error  # error: [unresolved-reference]

            # this diagnostic is correct: inferred type of `x` is `Literal[Color.GREEN]`
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

def if_else_exhaustive_no_assertion(x: A | B | C) -> int:
    if isinstance(x, A):
        return 0
    elif isinstance(x, B):
        return 1
    elif isinstance(x, C):
        return 2

def if_else_non_exhaustive(x: A | B | C):
    if isinstance(x, A):
        pass
    elif isinstance(x, C):
        pass
    else:
        this_should_be_an_error  # error: [unresolved-reference]

        # this diagnostic is correct: the inferred type of `x` is `B & ~A & ~C`
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
            no_diagnostic_here

            assert_never(x)

def match_exhaustive_no_assertion(x: A | B | C) -> int:
    match x:
        case A():
            return 0
        case B():
            return 1
        case C():
            return 2

def match_non_exhaustive(x: A | B | C):
    match x:
        case A():
            pass
        case C():
            pass
        case _:
            this_should_be_an_error  # error: [unresolved-reference]

            # this diagnostic is correct: the inferred type of `x` is `B & ~A & ~C`
            assert_never(x)  # error: [type-assertion-failure]
```
