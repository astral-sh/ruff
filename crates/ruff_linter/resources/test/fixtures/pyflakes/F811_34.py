"""Regression test for: https://github.com/astral-sh/ruff/issues/23802"""

# F811: both annotated assignments, first unused
bar: int = 1
bar: int = 2  # F811

x: str = "hello"
x: str = "world"  # F811

# OK: plain reassignment (no annotation)
y = 1
y = 2

# OK: first is plain, second is annotated
z = 1
z: int = 2

# OK: first is annotated, second is plain
w: int = 1
w = 2

# OK: used between assignments
a: int = 1
print(a)
a: int = 2
