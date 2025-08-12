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

## `isinstance` checks with generics

```toml
[environment]
python-version = "3.12"
```

```py
from typing import assert_never

class A[T]: ...
class ASub[T](A[T]): ...
class B[T]: ...
class C[T]: ...
class D: ...
class E: ...
class F: ...

def if_else_exhaustive(x: A[D] | B[E] | C[F]):
    if isinstance(x, A):
        pass
    elif isinstance(x, B):
        pass
    elif isinstance(x, C):
        pass
    else:
        # TODO: both of these are false positives (https://github.com/astral-sh/ty/issues/456)
        no_diagnostic_here  # error: [unresolved-reference]
        assert_never(x)  # error: [type-assertion-failure]

# TODO: false-positive diagnostic (https://github.com/astral-sh/ty/issues/456)
def if_else_exhaustive_no_assertion(x: A[D] | B[E] | C[F]) -> int:  # error: [invalid-return-type]
    if isinstance(x, A):
        return 0
    elif isinstance(x, B):
        return 1
    elif isinstance(x, C):
        return 2

def if_else_non_exhaustive(x: A[D] | B[E] | C[F]):
    if isinstance(x, A):
        pass
    elif isinstance(x, C):
        pass
    else:
        this_should_be_an_error  # error: [unresolved-reference]

        # this diagnostic is correct: the inferred type of `x` is `B[E] & ~A[D] & ~C[F]`
        assert_never(x)  # error: [type-assertion-failure]

def match_exhaustive(x: A[D] | B[E] | C[F]):
    match x:
        case A():
            pass
        case B():
            pass
        case C():
            pass
        case _:
            # TODO: both of these are false positives (https://github.com/astral-sh/ty/issues/456)
            no_diagnostic_here  # error: [unresolved-reference]
            assert_never(x)  # error: [type-assertion-failure]

# TODO: false-positive diagnostic (https://github.com/astral-sh/ty/issues/456)
def match_exhaustive_no_assertion(x: A[D] | B[E] | C[F]) -> int:  # error: [invalid-return-type]
    match x:
        case A():
            return 0
        case B():
            return 1
        case C():
            return 2

def match_non_exhaustive(x: A[D] | B[E] | C[F]):
    match x:
        case A():
            pass
        case C():
            pass
        case _:
            this_should_be_an_error  # error: [unresolved-reference]

            # this diagnostic is correct: the inferred type of `x` is `B[E] & ~A[D] & ~C[F]`
            assert_never(x)  # error: [type-assertion-failure]

# This function might seem a bit silly, but it's a pattern that exists in real-world code!
# see https://github.com/bokeh/bokeh/blob/adef0157284696ce86961b2089c75fddda53c15c/src/bokeh/core/property/container.py#L130-L140
def no_invalid_return_diagnostic_here_either[T](x: A[T]) -> ASub[T]:
    if isinstance(x, A):
        if isinstance(x, ASub):
            return x
        else:
            return ASub()
    else:
        # We *would* emit a diagnostic here complaining that it's an invalid `return` statement
        # ...except that we (correctly) infer that this branch is unreachable, so the complaint
        # is null and void (and therefore we don't emit a diagnostic)
        return x
```

## More `match` pattern types

### `as` patterns

```py
from typing import assert_never

def as_pattern_exhaustive(subject: int | str):
    match subject:
        case int() as x:
            pass
        case str() as y:
            pass
        case _:
            no_diagnostic_here

            assert_never(subject)

def as_pattern_non_exhaustive(subject: int | str):
    match subject:
        case int() as x:
            pass
        case _:
            this_should_be_an_error  # error: [unresolved-reference]

            # this diagnostic is correct: the inferred type of `subject` is `str`
            assert_never(subject)  # error: [type-assertion-failure]
```
