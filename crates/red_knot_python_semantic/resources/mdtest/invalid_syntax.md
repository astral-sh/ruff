# Syntax errors

Test cases to ensure that red knot does not panic if there are syntax errors in the source code.

The parser cannot recover from certain syntax errors completely which is why the number of syntax
errors could be more than expected in the following examples. For instance, if there's a keyword
(like `for`) in the middle of another statement (like function definition), then it's more likely
that the rest of the tokens are going to be part of the `for` statement and not the function
definition. But, it's not necessary that the remaining tokens are valid in the context of a `for`
statement.

## Keyword as identifiers

When keywords are used as identifiers, the parser recovers from this syntax error by emitting an
error and including the text value of the keyword to create the `Identifier` node.

### Name expression

#### Assignment

```py
# error: [invalid-syntax]
pass = 1
```

#### Type alias

```py
# error: [invalid-syntax]
# error: [invalid-syntax]
type pass = 1
```

#### Function definition

```py
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
def True(for):
    # error: [invalid-syntax]
    # error: [invalid-syntax]
    pass
```

#### For

```py
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [unresolved-reference] "Name `pass` used when not defined"
for while in pass:
    pass
```

#### While

```py
# error: [invalid-syntax]
# error: [unresolved-reference] "Name `in` used when not defined"
while in:
    pass
```

#### Match

```py
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [unresolved-reference] "Name `match` used when not defined"
match while:
    # error: [invalid-syntax]
    # error: [invalid-syntax]
    # error: [invalid-syntax]
    # error: [unresolved-reference] "Name `case` used when not defined"
    case in:
        # error: [invalid-syntax]
        # error: [invalid-syntax]
        pass
```

### Attribute expression

```py
# TODO: Check when support for attribute expressions is added

# error: [invalid-syntax]
# error: [unresolved-reference] "Name `foo` used when not defined"
for x in foo.pass:
    pass
```

## Invalid annotation

### `typing.Callable`

```py
from typing import Callable

# error: [invalid-syntax] "Expected index or slice expression"
# error: [invalid-type-form] "Special form `typing.Callable` expected exactly two arguments (parameter types and return type)"
def _(c: Callable[]):
    reveal_type(c)  # revealed: (...) -> Unknown
```
