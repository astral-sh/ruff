# Conflicting attributes and submodules

## Via import

```py
import a.b

reveal_type(a.b)  # revealed: <module 'a.b'>
```

`a/__init__.py`:

```py
b: int = 42
```

`a/b.py`:

```py
```

## Via from/import

```py
from a import b

reveal_type(b)  # revealed: int
```

`a/__init__.py`:

```py
b: int = 42
```

`a/b.py`:

```py
```

## Via both

```py
import a.b
from a import b

reveal_type(b)  # revealed: <module 'a.b'>
reveal_type(a.b)  # revealed: <module 'a.b'>
```

`a/__init__.py`:

```py
b: int = 42
```

`a/b.py`:

```py
```

## Via both (backwards)

In this test, we infer a different type for `b` than the runtime behavior of the Python interpreter.
The interpreter will not load the submodule `a.b` during the `from a import b` statement, since `a`
contains a non-module attribute named `b`. (See the [definition][from-import] of a `from...import`
statement for details.) However, because our import tracking is flow-insensitive, we will see that
`a.b` is imported somewhere in the file, and therefore assume that the `from...import` statement
sees the submodule as the value of `b` instead of the integer.

```py
from a import b
import a.b

# Python would say `int` for `b`
reveal_type(b)  # revealed: <module 'a.b'>
reveal_type(a.b)  # revealed: <module 'a.b'>
```

`a/__init__.py`:

```py
b: int = 42
```

`a/b.py`:

```py
```

[from-import]: https://docs.python.org/3/reference/simple_stmts.html#the-import-statement
