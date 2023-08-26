from typing import Set
from some.package.name import foo, bar


s = set()
s2 = {}
s3: set[int] = foo()

# these should match

# FURB132
if "x" in s:
    s.remove("x")


# FURB132
if "x" in s2:
    s2.remove("x")


# FURB132
if "x" in s3:
    s3.remove("x")


var = "y"
# FURB132
if var in s:
    s.remove(var)


if f"{var}:{var}" in s:
    s.remove(f"{var}:{var}")


def identity(x):
    return x


# TODO: FURB132
if identity("x") in s2:
    s2.remove(identity("x"))


# these should not

if "x" in s:
    s.remove("y")

s.discard("x")

s2 = set()

if "x" in s:
    s2.remove("x")

if "x" in s:
    s.remove("x")
    print("removed item")

if bar() in s:
    # bar() might have a side effect
    s.remove(bar())

if "x" in s:
    s.remove("x")
else:
    print("not found")

class Container:
    def remove(self, item) -> None:
        return

    def __contains__(self, other) -> bool:
        return True

c = Container()

if "x" in c:
    c.remove("x")
