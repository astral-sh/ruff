open("foo", "U")
open("foo", "Ur")
open("foo", "Ub")
open("foo", "rUb")
open("foo", "r")
open("foo", "rt")
open("f", "r", encoding="UTF-8")
open("f", "wt")

with open("foo", "U") as f:
    pass
with open("foo", "Ur") as f:
    pass
with open("foo", "Ub") as f:
    pass
with open("foo", "rUb") as f:
    pass
with open("foo", "r") as f:
    pass
with open("foo", "rt") as f:
    pass
with open("foo", "r", encoding="UTF-8") as f:
    pass
with open("foo", "wt") as f:
    pass

open(f("a", "b", "c"), "U")
open(f("a", "b", "c"), "Ub")

with open(f("a", "b", "c"), "U") as f:
    pass
with open(f("a", "b", "c"), "Ub") as f:
    pass

with open("foo", "U") as fa, open("bar", "U") as fb:
    pass
with open("foo", "Ub") as fa, open("bar", "Ub") as fb:
    pass

open("foo", mode="U")
open(name="foo", mode="U")
open(mode="U", name="foo")

with open("foo", mode="U") as f:
    pass
with open(name="foo", mode="U") as f:
    pass
with open(mode="U", name="foo") as f:
    pass

open("foo", mode="Ub")
open(name="foo", mode="Ub")
open(mode="Ub", name="foo")

with open("foo", mode="Ub") as f:
    pass
with open(name="foo", mode="Ub") as f:
    pass
with open(mode="Ub", name="foo") as f:
    pass

open(file="foo", mode='U', buffering=- 1, encoding=None, errors=None, newline=None, closefd=True, opener=None)
open(file="foo", buffering=- 1, encoding=None, errors=None, newline=None, closefd=True, opener=None, mode='U')
open(file="foo", buffering=- 1, encoding=None, errors=None, mode='U', newline=None, closefd=True, opener=None)
open(mode='U', file="foo", buffering=- 1, encoding=None, errors=None, newline=None, closefd=True, opener=None)

open(file="foo", mode='Ub', buffering=- 1, encoding=None, errors=None, newline=None, closefd=True, opener=None)
open(file="foo", buffering=- 1, encoding=None, errors=None, newline=None, closefd=True, opener=None, mode='Ub')
open(file="foo", buffering=- 1, encoding=None, errors=None, mode='Ub', newline=None, closefd=True, opener=None)
open(mode='Ub', file="foo", buffering=- 1, encoding=None, errors=None, newline=None, closefd=True, opener=None)

open = 1
open("foo", "U")
open("foo", "Ur")
open("foo", "Ub")
open("foo", "rUb")
open("foo", "r")
open("foo", "rt")
open("f", "r", encoding="UTF-8")
open("f", "wt")
