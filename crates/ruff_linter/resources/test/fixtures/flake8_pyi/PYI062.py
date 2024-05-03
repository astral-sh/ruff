from typing import Literal

x: Literal[True, False, True, False]  # PYI062 twice here

y: Literal[1, print("hello"), 3, Literal[4, 1]]  # PYI062 on the last 1

z: Literal[{1, 3, 5}, "foobar", {1,3,5}]  # PYI062 on the set literal

# Ensure issue is only raised once, even on nested literals
MyType = Literal["foo", Literal[True, False, True], "bar"]  # PYI062

n: Literal["No", "duplicates", "here", 1, "1"]
