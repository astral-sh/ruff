# Syntax errors

Test cases to ensure that red knot does not panic if there are syntax errors in the source code.

## Keyword as identifiers

When keywords are used as identifiers, the parser recovers from this syntax error by emitting an
error and including the text value of the keyword to create the `Identifier` node.

### Name expression

```py
# error: [invalid-syntax]
pass = 1

# error: [invalid-syntax]
# error: [invalid-syntax]
type pass = 1

# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
def True(for):
    # error: [invalid-syntax]
    pass

# TODO: Why is there two diagnostics for the same error?

# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [invalid-syntax]
# error: [unresolved-reference] "Name `pass` used when not defined"
# error: [unresolved-reference] "Name `pass` used when not defined"
for while in pass:
    pass

# error: [invalid-syntax]
# error: [unresolved-reference] "Name `in` used when not defined"
while in:
    pass

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
# TODO: Why is there two diagnostics for the same error?
# TODO: Check when support for attribute expressions is added

# error: [invalid-syntax]
# error: [unresolved-reference] "Name `foo` used when not defined"
# error: [unresolved-reference] "Name `foo` used when not defined"
for x in foo.pass:
    pass
```
