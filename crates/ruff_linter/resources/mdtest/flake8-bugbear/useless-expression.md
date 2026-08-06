# `useless-expression` (`B018`)

```toml
lint.preview = true
lint.select = ["B018"]
```

## String literals

In preview, string literals and f-strings are useless expressions unless Ruff
recognizes them as docstrings.

```py
"""Module docstring."""

"Standalone module string."  # error: [useless-expression]

value = 1
f"Useless module f-string: {value}"  # error: [useless-expression]


class Class:
    """Class docstring."""

    "Standalone class string."  # error: [useless-expression]
    f"Useless class f-string: {value}"  # error: [useless-expression]

    def method(self):
        """Method docstring."""

        local = 1
        "Standalone function string."  # error: [useless-expression]
        f"Useless function f-string: {local}"  # error: [useless-expression]
```

## Strings after overloads

A string after an overload is not a docstring for the preceding function.

```py
from typing import overload


@overload
def f(value: None) -> str: ...


"None overload documentation."  # error: [useless-expression]


@overload
def f(value: list[str]) -> int: ...


"List overload documentation."  # error: [useless-expression]


def f(value):
    return value
```

## Section separators

A standalone string used as a visual separator is still a useless expression.

```py
def main():
    pass


"""MAIN"""  # error: [useless-expression]

if __name__ == "__main__":
    main()
```

## Attribute docstrings

Ruff recognizes strings immediately following simple assignments and annotated
assignments at module or class scope as attribute docstrings. A second string
is not part of the attribute docstring.

```py
module_attribute = 1
"Module attribute docstring."

annotated_module_attribute: int
"Annotated module attribute docstring."


class Class:
    attribute = 1
    "Class attribute docstring."

    annotated_attribute: int
    "Annotated class attribute docstring."
    "Not an attribute docstring."  # error: [useless-expression]
```
