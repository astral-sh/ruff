# `pointless-string-statement` (`PLW0105`)

```toml
[lint]
preview = true
select = ["PLW0105"]
```

## Docstrings

A string as the first statement of a module, class, or function body is a real docstring and is
never flagged. This applies to any function, not just `__init__`.

```py
"""Module docstring."""


class Foo:
    """Class docstring."""

    def bar(self):
        """Function docstring."""
        return 1


def baz():
    """Another function docstring."""


async def qux():
    """Async function docstring."""
```

## Attribute docstrings

A string immediately following an assignment (including annotated assignments and type aliases) at
the module, class, or `__init__` level is an attribute docstring and is not flagged.

```py
x = 1
"""Docstring for `x`."""

y: int = 2
"""Docstring for `y`."""

type Alias = int
"""Docstring for `Alias`."""


class Foo:
    attr = 1
    """Docstring for `attr`."""

    def __init__(self):
        self.value = 2
        """Docstring for `value`."""
```

## Strings after assignments in other functions

Outside `__init__`, a string after an assignment is not an attribute docstring and is flagged.

```py
class Foo:
    def method(self):
        self.x = 1
        "Not an attribute docstring."  # error: [pointless-string-statement]
```

## Strings in nested blocks

Nested blocks like `if` bodies have no docstring concept, so a string as the first statement of
such a block is flagged.

```py
if True:
    "Not a docstring."  # error: [pointless-string-statement]


def foo():
    for _ in range(3):
        "Not a docstring."  # error: [pointless-string-statement]
```

However, a string following an assignment inside a nested block is still an attribute docstring if
the enclosing scope is a module, class, or `__init__` method.

```py
import sys

if sys.platform == "linux":
    x = 1
    """Docstring for `x`."""
```

## Misplaced module docstrings

A string that follows any non-assignment statement, such as an import, is flagged.

```py
import sys

"Misplaced module docstring."  # snapshot: pointless-string-statement
```

```snapshot
error[PLW0105]: String statement has no effect
 --> src/mdtest_snippet.py:3:1
  |
3 | "Misplaced module docstring."  # snapshot: pointless-string-statement
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
```

## Statements that only look like assignments

Only `Assign`, `AnnAssign`, and `TypeAlias` introduce an attribute docstring. An augmented
assignment does not.

```py
count = 0
count += 1
"Not an attribute docstring."  # error: [pointless-string-statement]
```

A bare annotation with no value does introduce one, matching pylint.

```py
x: int
"Docstring for `x`."
```

## A second string after the real docstring

A second string following the real docstring is also flagged, matching pylint. This deviates from
PEP 257, which recognizes such strings as "additional docstrings"; pylint flags them because its
parser only exposes the first string as the docstring, and this rule preserves that behavior for
parity.

```py
"""Module docstring."""

"Second string."  # error: [pointless-string-statement]
```

## Pylint's `statement_without_effect` functional test

Ported from pylint's `tests/functional/s/statement_without_effect.py`. Only the
`pointless-string-statement` expectations apply; interleaved statements exercising other pylint
messages are kept for fidelity to the upstream fixture, except the file's trailing functions
(which only exercise pylint's `pointless-statement` and `pointless-exception-statement`) are
omitted.

```py
"""Test for statements without effects."""

"""inline doc string should use a separated message"""  # error: [pointless-string-statement]

__revision__ = ""

__revision__

__revision__ <= 1

__revision__.lower()

[i for i in __revision__]

"""inline doc string should use a separated message"""  # error: [pointless-string-statement]


__revision__.lower()

list() and tuple()


def to_be():
    """return 42"""
    return "42"


ANSWER = to_be()
ANSWER == to_be()

to_be() or not to_be()
to_be().title

GOOD_ATTRIBUTE_DOCSTRING = 42
"""Module level attribute docstring is fine. """


class ClassLevelAttributeTest:
    """test attribute docstrings."""

    class ClassLevelException(Exception):
        """Exception defined for access as a class attribute."""

    good_attribute_docstring = 24
    """ class level attribute docstring is fine either. """
    second_good_attribute_docstring = 42
    # Comments are good.

    # empty lines are good, too.
    """ Still a valid class level attribute docstring. """

    def __init__(self):
        self.attr = 42
        """ Good attribute docstring """
        attr = 24
        """ Still a good __init__ level attribute docstring. """
        val = 0
        for val in range(42):
            val += attr
        """ Invalid attribute docstring """  # error: [pointless-string-statement]
        self.val = val

    def test(self):
        """invalid attribute docstrings here."""
        self.val = 42
        """ this is an invalid attribute docstring. """  # error: [pointless-string-statement]
```

## Pylint's `statement_without_effect_py36` functional test

Ported from pylint's `tests/functional/s/statement_without_effect_py36.py`.

```py
"""Test for statements without effects."""


class ClassLevelAttributeTest:
    """test attribute docstrings."""

    some_variable: int = 42
    """Data docstring"""

    some_other_variable: int = 42
    """Data docstring"""

    def func(self):
        """Some Empty Docstring"""

    """useless"""  # error: [pointless-string-statement]
```

## Non-string constants and f-strings

Only plain string literals are flagged. Other pointless constants are covered by other rules
(e.g. pylint's `pointless-statement`).

```py
42
b"bytes statement"
f"f-string statement"
t"template statement"
```

## Notebook cells

A string that is the last top-level expression in a notebook cell is the cell's displayed output,
so it is not flagged. Strings elsewhere in a cell are still flagged, as is a string that ends a
cell from inside a nested block, since only a top-level expression is displayed.

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
      "source": ["\"not the cell's output\"  # error: [pointless-string-statement]\n", "print(x)"]
    },
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": ["if x:\n", "    \"ends the cell but is nested\"  # error: [pointless-string-statement]"]
    }
  ],
  "metadata": {},
  "nbformat": 4,
  "nbformat_minor": 4
}
```
