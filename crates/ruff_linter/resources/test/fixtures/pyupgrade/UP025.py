u"Hello"

x = u"Hello"  # UP025

u'world'  # UP025

print(u"Hello")  # UP025

print(u'world')  # UP025

import foo

foo(u"Hello", U"world", a=u"Hello", b=u"world")  # UP025

# Retain quotes when fixing.
x = u'hello'  # UP025
x = u"""hello"""  # UP025
x = u'''hello'''  # UP025
x = u'Hello "World"'  # UP025

u = "Hello"  # OK
u = u  # OK

def hello():
    return"Hello"  # OK

f"foo"u"bar"  # OK
f"foo" u"bar"  # OK

# https://github.com/astral-sh/ruff/issues/18895
""u""
""u"hi"
""""""""""""""""""""u"hi"
""U"helloooo"
# https://github.com/astral-sh/ruff/issues/10586
# A unicode prefix inside a string type definition (forward reference) is part
# of the type expression and must not be flagged.
import typing_extensions as te

A: "te.Literal[u'x', '\r\n', '\r']" = "\n"
B: list["te.Literal[u'x']"] = []


def f(x: "te.Literal[u'x']") -> "te.Literal[u'y']":
    return u"y"  # UP025 (runtime string, outside the annotation)
