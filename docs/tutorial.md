# Tutorial

This tutorial will walk you through the process of integrating Ruff into your project. For a more
detailed overview, see [_Configuration_](configuration.md).

## Getting Started

Let's assume that our project structure looks like:

```text
numbers
  ├── __init__.py
  └── numbers.py
```

Where `numbers.py` contains the following code:

```py
from typing import List

import os


def sum_even_numbers(numbers: List[int]) -> int:
    """Given a list of integers, return the sum of all even numbers in the list."""
    return sum(num for num in numbers if num % 2 == 0)
```

To start, we'll install Ruff through PyPI (or with your [preferred package manager](installation.md)):

```shell
> pip install ruff
```

We can then run Ruff over our project via:

```shell
❯ ruff check .
numbers/numbers.py:3:8: F401 [*] `os` imported but unused
Found 1 error.
[*] 1 potentially fixable with the --fix option.
```

Ruff identified an unused import, which is a common error in Python code. Ruff considers this a
"fixable" error, so we can resolve the issue automatically by running:

```shell
❯ ruff check --fix .
Found 1 error (1 fixed, 0 remaining).
```

Running `git diff` shows the following:

```diff
--- a/numbers/numbers.py
+++ b/numbers/numbers.py
@@ -1,7 +1,5 @@
 from typing import List

-import os
-

def sum_even_numbers(numbers: List[int]) -> int:
    """Given a list of integers, return the sum of all even numbers in the list."""
    return sum(num for num in numbers if num % 2 == 0)
```

Thus far, we've been using Ruff's default configuration. Let's take a look at how we can customize
Ruff's behavior.

## Configuration

To determine the appropriate settings for each Python file, Ruff looks for the first
`pyproject.toml`, `ruff.toml`, or `.ruff.toml` file in the file's directory or any parent directory.

Let's create a `pyproject.toml` file in our project's root directory:

```toml
[tool.ruff]
# Decrease the maximum line length to 79 characters.
line-length = 79
```

Running Ruff again, we can see that it now enforces a line length of 79 characters:

```shell
❯ ruff check .
numbers/numbers.py:6:80: E501 Line too long (83 > 79 characters)
Found 1 error.
```

For a full enumeration of the supported settings, see [_Settings_](settings.md). For our project
specifically, we'll want to make note of the minimum supported Python version:

```toml
[project]
# Support Python 3.10+.
requires-python = ">=3.10"

[tool.ruff]
# Decrease the maximum line length to 79 characters.
line-length = 79
src = ["src"]
```

### Rule Selection

Ruff supports [over 500 lint rules](rules.md) split across over 40 built-in plugins, but
determining the right set of rules will depend on your project's needs: some rules may be too
strict, some are framework-specific, and so on.

By default, Ruff enforces the `E`- and `F`-prefixed rules, which correspond to those derived from
pycodestyle and Pyflakes, respectively.

If you're introducing a linter for the first time, **the default rule set is a great place to
start**: it's narrow and focused while catching a wide variety of common errors (like unused
imports) with zero configuration.

If you're migrating to Ruff from another linter, you can enable rules that are equivalent to
those enforced in your previous configuration. For example, if we want to enforce the pyupgrade
rules, we can add the following to our `pyproject.toml`:

```toml
[tool.ruff]
select = [
  "E",   # pycodestyle
  "F",   # pyflakes
  "UP",  # pyupgrade
]
```

If we run Ruff again, we'll see that it now enforces the pyupgrade rules. In particular, Ruff flags
the use of `List` instead of its standard-library variant:

```shell
❯ ruff check .
numbers/numbers.py:5:31: UP006 [*] Use `list` instead of `List` for type annotations
numbers/numbers.py:6:80: E501 Line too long (83 > 79 characters)
Found 2 errors.
[*] 1 potentially fixable with the --fix option.
```

Over time, we may choose to enforce additional rules. For example, we may want to enforce that
all functions have docstrings:

```toml
[tool.ruff]
select = [
  "E",   # pycodestyle
  "F",   # pyflakes
  "UP",  # pyupgrade
  "D",   # pydocstyle
]

[tool.ruff.pydocstyle]
convention = "google"
```

If we run Ruff again, we'll see that it now enforces the pydocstyle rules:

```shell
❯ ruff check .
numbers/__init__.py:1:1: D104 Missing docstring in public package
numbers/numbers.py:1:1: D100 Missing docstring in public module
numbers/numbers.py:5:31: UP006 [*] Use `list` instead of `List` for type annotations
numbers/numbers.py:5:80: E501 Line too long (83 > 79 characters)
Found 3 errors.
[*] 1 potentially fixable with the --fix option.
```

### Ignoring Errors

Any lint rule can be ignored by adding a `# noqa` comment to the line in question. For example,
let's ignore the `UP006` rule for the `List` import:

```py
from typing import List


def sum_even_numbers(numbers: List[int]) -> int:  # noqa: UP006
    """Given a list of integers, return the sum of all even numbers in the list."""
    return sum(num for num in numbers if num % 2 == 0)
```

Running Ruff again, we'll see that it no longer flags the `List` import:

```shell
❯ ruff check .
numbers/__init__.py:1:1: D104 Missing docstring in public package
numbers/numbers.py:1:1: D100 Missing docstring in public module
numbers/numbers.py:5:80: E501 Line too long (83 > 79 characters)
Found 3 errors.
```

If we want to ignore a rule for an entire file, we can add a `# ruff: noqa` comment to the top of
the file:

```py
# ruff: noqa: UP006
from typing import List


def sum_even_numbers(numbers: List[int]) -> int:
    """Given a list of integers, return the sum of all even numbers in the list."""
    return sum(num for num in numbers if num % 2 == 0)
```

When enabling a new rule on an existing codebase, you may want to ignore all _existing_
violations of that rule and instead focus on enforcing it going forward.

Ruff enables this workflow via the `--add-noqa` flag, which will adds a `# noqa` directive to each
line based on its existing violations. We can combine `--add-noqa` with the `--select` command-line
flag to add `# noqa` directives to all existing `UP006` violations:

```shell
❯ ruff check --select UP006 --add-noqa .
Added 1 noqa directive.
```

Running `git diff` shows the following:

```diff
diff --git a/tutorial/src/main.py b/tutorial/src/main.py
index b9291c5ca..b9f15b8c1 100644
--- a/numbers/numbers.py
+++ b/numbers/numbers.py
@@ -1,6 +1,6 @@
 from typing import List


-def sum_even_numbers(numbers: List[int]) -> int:
+def sum_even_numbers(numbers: List[int]) -> int:  # noqa: UP006
     """Given a list of integers, return the sum of all even numbers in the list."""
     return sum(num for num in numbers if num % 2 == 0)
```

## Continuous Integration

This tutorial has focused on Ruff's command-line interface, but Ruff can also be used as a
[pre-commit](https://pre-commit.com) hook:

```yaml
- repo: https://github.com/astral-sh/ruff-pre-commit
  # Ruff version.
  rev: v0.0.276
  hooks:
    - id: ruff
```

See [_Usage_](usage.md) for more.

## Editor Integrations

Ruff can also be used as a [VS Code extension](https://github.com/astral-sh/ruff-vscode) or
alongside any other editor through the [Ruff LSP](https://github.com/astral-sh/ruff-lsp).

See [_Editor Integrations_](editor-integrations.md).
