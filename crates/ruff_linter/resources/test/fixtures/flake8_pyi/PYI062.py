from typing import Literal
import typing as t
import typing_extensions

x: Literal[True, False, True, False]  # PYI062 twice here

y: Literal[1, print("hello"), 3, Literal[4, 1]]  # PYI062 on the last 1

z: Literal[{1, 3, 5}, "foobar", {1,3,5}]  # PYI062 on the set literal

Literal[1, Literal[1]]  # once
Literal[1, 2, Literal[1, 2]]  # twice
Literal[1, Literal[1], Literal[1]]  # twice
Literal[1, Literal[2], Literal[2]]  # once
t.Literal[1, t.Literal[2, t.Literal[1]]]  # once
typing_extensions.Literal[1, 1, 1]  # twice
Literal[
    1, # comment
    Literal[ # another comment
        1
    ]
]  # once

# Ensure issue is only raised once, even on nested literals
MyType = Literal["foo", Literal[True, False, True], "bar"]  # PYI062

n: Literal["No", "duplicates", "here", 1, "1"]


# nested literals, all equivalent to `Literal[1]`
Literal[Literal[1]]  # no duplicate
Literal[Literal[Literal[1], Literal[1]]]  # once
Literal[Literal[1], Literal[Literal[Literal[1]]]]  # once


# https://github.com/astral-sh/ruff/pull/27714
# Dynamic (non-literal) duplicate members: only reachable for a syntactically-invalid `Literal`,
# but `items[0]` evaluates `__getitem__` each time, so removing a "duplicate" may change behavior.
# The diagnostic is still raised, but its fix must be marked unsafe.
Literal[items[0], items[0]]  # once, unsafe fix
Literal[foo(), foo()]  # once, unsafe fix
