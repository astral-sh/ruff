# `call-non-callable` (`PLE1102`)

```toml
[lint]
preview = true
select = ["PLE1102"]
```

## Callables

Cases of callables that shouldn't trigger a diagnostic.
The only guaranteed callable here is `lambda` in all other cases we just can't be certain and therefore allow it.

```python

# Lambda
(lambda: None)()

# Name
foo()

# Call
foo()()

# Attribute
foo.bar()

# Subscript
foo[0]()

# Await
async def f():
    (await foo())()

```

## Errors + SyntaxWarnings

All basic cases of non-callables, all emit SyntaxWarning error during Python compilation and result in `TypeError` at runtime.

```python
# String
"foo"()  # error: [call-non-callable]

# F-string
f"foo"()  # error: [call-non-callable]

# Bytes
b"foo"()  # error: [call-non-callable]

# Number (int)
(1)()  # error: [call-non-callable]

# Number (float)
(1.5)()  # error: [call-non-callable]

# Number (complex)
(1j)()  # error: [call-non-callable]

# Number (bool)
True()  # error: [call-non-callable]

# None
None()  # error: [call-non-callable]

# Ellipsis
(...)()  # error: [call-non-callable]

# Dict
{"a": 1}()  # error: [call-non-callable]

# Dict comprehension
{k: v for k, v in [("a", 1)]}()  # error: [call-non-callable]

# List
[1, 2, 3]()  # error: [call-non-callable]

# List comprehension
[i for i in range(5)]()  # error: [call-non-callable]

# Set
{1, 2, 3}()  # error: [call-non-callable]

# Set comprehension
{i for i in range(5)}()  # error: [call-non-callable]

# Tuple
(1, 2, 3)()  # error: [call-non-callable]

# Generator
(i for i in range(5))()  # error: [call-non-callable]

# Template
t"hello {name}"()  # error: [call-non-callable]

```
