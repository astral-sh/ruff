from typing import Literal

x: Literal[True, False, True, False]  # PY062 twice here

y: Literal[1, print("hello"), 3, Literal[4, 1]]  # PY062 on the last 1

z: Literal[{1, 3, 5}, "foobar", {1,3,5}]  # PY062 on the set literal

n: Literal["No", "duplicates", "here", 1, "1"]
