# `useless-expression` (`B018`)

```toml
lint.preview = true
lint.select = ["B018"]
```

## String literals

In preview, string literals and side-effect-free f-strings are useless
expressions unless Ruff recognizes them as docstrings.

```py
"""Module docstring."""

"Standalone module string."  # error: [useless-expression]

f"Useless module f-string: {1}"  # error: [useless-expression]


class Class:
    """Class docstring."""

    "Standalone class string."  # error: [useless-expression]
    f"Useless class f-string: {1}"  # error: [useless-expression]

    def method(self):
        """Method docstring."""

        "Standalone function string."  # error: [useless-expression]
        f"Useless function f-string: {1}"  # error: [useless-expression]
```

## F-string formatting side effects

Interpolating a value can call a user-defined `__format__` method, including
when the value appears inside another interpolation's format specification.
These f-strings are not useless expressions because formatting the value has
an observable side effect.

```py
class Formatted:
    def __format__(self, spec: str) -> str:
        print("formatted")
        return "1"


value = Formatted()
f"{value}"
f"{1:{value}}"
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

Ruff recognizes strings immediately following simple assignments, annotated
assignments, and `type` statements at module or class scope as attribute
docstrings. A second string is not part of the attribute docstring.

```py
module_attribute = 1
"Module attribute docstring."

annotated_module_attribute: int
"Annotated module attribute docstring."

type ModuleAlias = int
"Module type alias docstring."


class Class:
    attribute = 1
    "Class attribute docstring."

    annotated_attribute: int
    "Annotated class attribute docstring."

    type ClassAlias = str
    "Class type alias docstring."

    "Not an attribute docstring."  # error: [useless-expression]


def function():
    type LocalAlias = bytes
    "Not an attribute docstring."  # error: [useless-expression]
```

## Instance attribute docstrings

Strings following instance attribute assignments directly in `__init__` are
attribute docstrings. Local assignments, nested assignments, and assignments in
other methods do not introduce attribute docstrings.

```py
class Class:
    def __init__(this):
        this.attribute = 1
        "Instance attribute docstring."

        this.annotated_attribute: int
        "Annotated instance attribute docstring."

        local = 1
        "Not an instance attribute docstring."  # error: [useless-expression]

        if local:
            this.nested_attribute = 1
            "Not a top-level assignment."  # error: [useless-expression]

        this = object()
        this.rebound_attribute = 1
        "Not an instance attribute docstring."  # error: [useless-expression]

    def method(self):
        self.attribute = 1
        "Not in an `__init__` method."  # error: [useless-expression]
```
