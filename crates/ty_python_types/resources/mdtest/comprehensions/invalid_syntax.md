# Comprehensions with invalid syntax

```py
# Missing `in` keyword.

# It's reasonably clear here what they *meant* to write,
# so we'll still infer the correct type:

# error: [invalid-syntax] "Expected `in`, found name"
# revealed: int
[reveal_type(a) for a range(3)]


# Missing iteration variable

# error: [invalid-syntax] "Expected an identifier, but found a keyword `in` that cannot be used here"
# error: [invalid-syntax] "Expected `in`, found name"
# error: [unresolved-reference]
# revealed: Unknown
[reveal_type(b) for in range(3)]


# Missing iterable

# error: [invalid-syntax] "Expected an expression"
# revealed: Unknown
[reveal_type(c) for c in]


# Missing `in` keyword and missing iterable

# error: [invalid-syntax] "Expected `in`, found `]`"
# revealed: Unknown
[reveal_type(d) for d]
```
