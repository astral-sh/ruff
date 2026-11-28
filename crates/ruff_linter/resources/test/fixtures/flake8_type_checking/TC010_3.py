from typing import TypeVar, TYPE_CHECKING

import foo

if TYPE_CHECKING:
    from foo import Foo


x: "Foo" | str  # TC010
x: ("int" | str) | "Foo"  # TC010


def func():
    x: "Foo" | str  # OK


z: list[str, str | "list[str]"] = []  # TC010

type A = Value["Foo" | str]  # OK

OldS = TypeVar('OldS', int | 'foo.Bar', str)  # TC010

x: ("Foo"  # TC010 (unsafe fix)
    " | str" | None)

OldT = TypeVar("OldT", ("Foo"  # TC010 (unsafe fix)
                        " | str" | None))

x: """int |
str""" | None  # TC010 (tricky multiline string case)

# Escaped newlines: https://github.com/astral-sh/ruff/issues/30957
x: """\nint # comment\n""" | None  # TC010

# Trailing comments
x: """\nint # comment""" | None  # TC010
x: "int # comment" | None  # TC010
