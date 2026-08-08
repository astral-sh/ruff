# `useless-expression` (`B018`)

In preview, B018 flags standalone string and f-string literals that are not docstrings or
attribute docstrings. The sections below cover the string handling.

```toml
[lint]
preview = true
select = ["B018"]
```

## Docstrings

A string as the first statement of a module, class, or function body is a docstring and is never
flagged.

```py
"""Module docstring."""


class Foo:
    """Class docstring."""

    def bar(self):
        """Method docstring."""
        return 1


def baz():
    """Function docstring."""
```

## Attribute docstrings

A string immediately following an assignment, annotated assignment, or type alias at the module,
class, or `__init__` method level is an attribute docstring and is not flagged. The exemption
accepts any assignment target and valueless annotations, intentionally broader than PEP 257's
"simple assignment" to stay conservative.

```py
x = 1
"""Docstring for `x`."""

y: int = 2
"""Docstring for `y`."""

length: int
"""Docstring for `length`, which has no value yet."""

type Alias = int
"""Docstring for `Alias`."""

a, b = 1, 2
"""Docstring for `a` and `b`."""


class Foo:
    attr = 1
    """Docstring for `attr`."""

    typed: int = 2
    """Docstring for `typed`."""

    def __init__(self):
        self.value = 3
        """Docstring for `value`."""
        self.typed: int = 4
        """Docstring for `typed`."""
        local = 5
        """Docstring for `local`."""
```

Comments and blank lines between the assignment and the string don't break the association.

```py
class Foo:
    attr = 1
    # A comment.

    # Another comment.
    """Still a docstring for `attr`."""
```

## Strings after assignments in other functions

Outside `__init__`, a string after an assignment is not an attribute docstring and is flagged.

```py
class Foo:
    def method(self):
        self.x = 1
        "Not an attribute docstring."  # error: [useless-expression]
```

## Strings in nested blocks

Nested blocks have no docstring concept, so a string as the first statement of such a block is
flagged.

```py
if True:
    "Not a docstring."  # error: [useless-expression]


def foo():
    for _ in range(3):
        "Not a docstring."  # error: [useless-expression]
```

However, a string following an assignment inside a nested block is still an attribute docstring if
the enclosing scope is a module, class, or `__init__` method.

```py
import sys

if sys.platform == "linux":
    x = 1
    """Docstring for `x`."""


class Foo:
    def __init__(self, flag):
        if flag:
            self.x = 1
            """Docstring for `x`."""
```

## An assignment's docstring expectation doesn't escape its block

Only a string immediately following the assignment, in the same block, is an attribute docstring.
A string after the enclosing block, or in a sibling `else` clause, is flagged even when the block
ends with an assignment.

```py
class Foo:
    if True:
        x = 1
    "Not an attribute docstring."  # error: [useless-expression]
```

```py
if True:
    x = 1
else:
    "Not an attribute docstring."  # error: [useless-expression]
```

## Misplaced module docstrings

A string that follows any non-assignment statement, such as an import, is flagged.

```py
import sys

"Misplaced module docstring."  # snapshot: useless-expression
```

```snapshot
error[B018]: Found useless string statement. Either convert it to a comment or remove it.
 --> src/mdtest_snippet.py:3:1
  |
3 | "Misplaced module docstring."  # snapshot: useless-expression
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

## A second string after the docstring

A string immediately following the real docstring is flagged. PEP 257 calls these "additional
docstrings", but they are discarded at runtime, and pylint's `pointless-string-statement` flags
them too.

```py
"""Module docstring."""

"Second string."  # error: [useless-expression]
```

## F-strings

An f-string is never a docstring, so it's flagged even in docstring position, unless it has side
effects.

```py
def foo():
    f"Not a docstring."  # error: [useless-expression]


def bar():
    x = 1
    f"{x} interpolated"  # error: [useless-expression]


def baz():
    f"{print('side effect')}"
```

## Stable behavior

Without preview, string and f-string statements are skipped entirely.

```toml
[lint]
select = ["B018"]
```

```py
import sys

"Misplaced module docstring, skipped on stable."

f"Bare f-string, skipped on stable."
```

## Notebook cells

A string that is the last top-level expression in a notebook cell is the cell's displayed output,
so it is not flagged, matching B018's existing behavior for other expressions. Strings elsewhere
in a cell, including a cell-final string nested in a block, are still flagged.

```toml
[lint]
preview = true
select = ["B018"]
```

`notebook.ipynb`:

```ipynb
{
  "cells": [
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["x = 1\n", "print(x)\n", "\"rendered as the cell's output\""]
    },
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["\"not the cell's output\"  # error: [useless-expression]\n", "print(x)"]
    },
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["if x:\n", "    \"ends the cell but is nested\"  # error: [useless-expression]"]
    }
  ],
  "metadata": {},
  "nbformat": 4,
  "nbformat_minor": 4
}
```
