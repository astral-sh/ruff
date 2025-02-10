from typing import Literal
import typing as t
import typing_extensions


y: Literal[1, print("hello"), 3, Literal[4, 1]]
Literal[1, Literal[1]]
Literal[1, 2, Literal[1, 2]]
Literal[1, Literal[1], Literal[1]]
Literal[1, Literal[2], Literal[2]]
t.Literal[1, t.Literal[2, t.Literal[1]]]
Literal[
    1, # comment 1
    Literal[ # another comment
        1 # yet another comment
    ]
]  # once

# Ensure issue is only raised once, even on nested literals
MyType = Literal["foo", Literal[True, False, True], "bar"]

# nested literals, all equivalent to `Literal[1]`
Literal[Literal[1]]
Literal[Literal[Literal[1], Literal[1]]]
Literal[Literal[1], Literal[Literal[Literal[1]]]]

# OK
x: Literal[True, False, True, False]
z: Literal[{1, 3, 5}, "foobar", {1,3,5}]
typing_extensions.Literal[1, 1, 1]
n: Literal["No", "duplicates", "here", 1, "1"]
