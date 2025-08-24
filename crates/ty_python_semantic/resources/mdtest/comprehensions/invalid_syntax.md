# Comprehensions with invalid syntax

```py
class IntIterator:
    def __next__(self) -> int:
        return 42

class IntIterable:
    def __iter__(self) -> IntIterator:
        return IntIterator()

# Missing 'in' keyword.

# It's reasonably clear here what they *meant* to write,
# so we'll still infer the correct type:

# error: [invalid-syntax] "Expected 'in', found name"
# revealed: int
[reveal_type(a) for a IntIterable()]


# Missing iteration variable

# error: [invalid-syntax] "Expected an identifier, but found a keyword 'in' that cannot be used here"
# error: [invalid-syntax] "Expected 'in', found name"
# error: [unresolved-reference]
# revealed: Unknown
[reveal_type(b) for in IntIterable()]


# Missing iterable

# error: [invalid-syntax] "Expected an expression"
# revealed: Unknown
[reveal_type(c) for c in]


# Missing 'in' keyword and missing iterable

# error: [invalid-syntax] "Expected 'in', found ']'"
# revealed: Unknown
[reveal_type(d) for d]
```
