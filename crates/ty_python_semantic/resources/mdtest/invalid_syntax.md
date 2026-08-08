# Syntax errors

Test cases to ensure that ty does not panic if there are syntax errors in the source code.

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

## Invalid assignment expression target

Parser recovery can produce a named expression target that is not a name. If that named expression
is used as part of a member expression, we should report the syntax error without treating it as a
valid place.

```py
obj = 1

# error: [invalid-syntax] "Assignment expression target must be an identifier"
out = (obj.attr := obj).attr

# error: [invalid-syntax] "Assignment expression target must be an identifier"
out = (obj[0] := obj).attr
```

## Match-pattern alternatives binding different names

A capture present in only one invalid `or` alternative is possibly undefined.

```py
match 0:
    case first | second:  # error: [invalid-syntax] "alternative patterns bind different names"
        first  # error: [possibly-unresolved-reference]
        second  # error: [possibly-unresolved-reference]
```

## Match-pattern alternative without a binding

A capture missing from one alternative is possibly undefined, regardless of alternative order.

```py
match (0,):
    # error: [invalid-syntax] "alternative patterns bind different names"
    case [first_value] | []:
        first_value  # error: [possibly-unresolved-reference]
```

An alternative without a capture can also occur first.

```py
match (0,):
    # error: [invalid-syntax] "alternative patterns bind different names"
    case [] | [last_value]:
        last_value  # error: [possibly-unresolved-reference]
```

A capture limited to the middle of three alternatives also remains possibly undefined.

```py
match (0,):
    # error: [invalid-syntax] "alternative patterns bind different names"
    case [] | [middle_value] | []:
        middle_value  # error: [possibly-unresolved-reference]
```

## Previously bound match-pattern captures

A prior binding remains visible on alternatives that do not capture the name.

```py
value = "previous"

match (0,):
    case [value] | []:  # error: [invalid-syntax] "alternative patterns bind different names"
        value

value
```

## Partially overlapping match-pattern bindings

Shared captures remain definitely bound; branch-specific captures are possibly undefined.

```py
match (0, 1):
    # error: [invalid-syntax] "alternative patterns bind different names"
    case [first, shared] | [second, shared]:
        first  # error: [possibly-unresolved-reference]
        second  # error: [possibly-unresolved-reference]
        shared
```

## Nested mismatched match-pattern bindings

Syntax checking stops after an outer mismatch, but unchecked nested alternatives must still be
modeled safely.

```py
match (0,):
    # error: [invalid-syntax] "alternative patterns bind different names"
    case [first] | [second] | [third | fourth]:
        third  # error: [possibly-unresolved-reference]
        fourth  # error: [possibly-unresolved-reference]
```

## Partially bound match-pattern capture in a guard

A guard can observe a name that is bound by only one invalid alternative.

```py
match (0,):
    # error: [invalid-syntax] "alternative patterns bind different names"
    # error: [possibly-unresolved-reference]
    case [value] | [] if value:
        pass
```

## Malformed match-case recovery

Parser recovery treats the trailing name as an annotation-only statement, whose binding lookup must
not panic.

```py
match 0:
    # error: [invalid-syntax] "alternative patterns bind different names"
    # error: [invalid-syntax] "Expected `:`, found name"
    # error: [invalid-syntax] "Expected an expression"
    case first | second first:
```

## Invalid annotation

### `typing.Callable`

```py
from typing import Callable

# error: [invalid-syntax] "Expected index or slice expression"
# error: [invalid-type-form] "Special form `Callable` expected exactly two arguments (parameter types and return type)"
def _(c: Callable[]):
    reveal_type(c)  # revealed: (...) -> Unknown
```

### `typing.Tuple`

```py
from typing import Tuple

# error: [invalid-syntax] "Expected index or slice expression"
InvalidEmptyTuple = Tuple[]

def _(t: InvalidEmptyTuple):
    reveal_type(t)  # revealed: tuple[Unknown]
```

### `typing.Union`

```py
from typing import Union

# error: [invalid-syntax] "Expected index or slice expression"
InvalidEmptyUnion = Union[]

def _(u: InvalidEmptyUnion):
    reveal_type(u)  # revealed: Unknown
```

### `typing.Unpack`

```toml
[environment]
python-version = "3.11"
```

An empty `Unpack` nested inside a union and a generic specialization should report its syntax error
without panicking.

```py
from typing import Union, Unpack

# error: [invalid-syntax] "Expected index or slice expression"
list[Union[Unpack[], None]]
```

### `typing.Annotated`

```py
from typing import Annotated

# error: [invalid-syntax] "Expected index or slice expression"
# error: [invalid-type-form] "Special form `typing.Annotated` expected at least 2 arguments (one type and at least one metadata element)"
InvalidEmptyAnnotated = Annotated[]

def _(a: InvalidEmptyAnnotated):
    reveal_type(a)  # revealed: Unknown
```
