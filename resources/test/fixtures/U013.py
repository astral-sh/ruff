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