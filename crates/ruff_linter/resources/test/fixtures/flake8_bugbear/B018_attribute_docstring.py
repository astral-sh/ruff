# These test cases not only check for `B018` but also verifies that the semantic model
# correctly identifies certain strings as attribute docstring. And, by way of not
# raising the `B018` violation, it can be verified.

a: int
"a: docstring"

b = 1
"b: docstring" " continue"
"b: not a docstring"

c: int = 1
"c: docstring"

_a: int
"_a: docstring"

if True:
    d = 1
    "d: not a docstring"

(e := 1)
"e: not a docstring"

f = 0
f += 1
"f: not a docstring"

g.h = 1
"g.h: not a docstring"

(i) = 1
"i: docstring"

(j): int = 1
"j: docstring"

(k): int
"k: docstring"

l = m = 1
"l m: not a docstring"

n.a = n.b = n.c = 1
"n.*: not a docstring"

(o, p) = (1, 2)
"o p: not a docstring"

[q, r] = [1, 2]
"q r: not a docstring"

*s = 1
"s: not a docstring"


class Foo:
    a = 1
    "Foo.a: docstring"

    b: int
    "Foo.b: docstring"
    "Foo.b: not a docstring"

    c: int = 1
    "Foo.c: docstring"

    def __init__(self) -> None:
        # This is actually a docstring but we currently don't detect it.
        self.x = 1
        "self.x: not a docstring"

        t = 2
        "t: not a docstring"

    def random(self):
        self.y = 2
        "self.y: not a docstring"

        u = 2
        "u: not a docstring"

    def add(self, y: int):
        self.x += y


def function():
    v = 2
    "v: not a docstring"


function.a = 1
"function.a: not a docstring"
