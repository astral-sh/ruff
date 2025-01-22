from enum import Enum


### Errors

E = Enum("E", "A B C")
E = Enum("E", "A,B,C")
E = Enum("E", "")
E = Enum("E", " A B, C")

E = Enum("E", ["A", "B", "C"])
E = Enum("E", ("A", "B", "C"))
E = Enum("E", [("A", 1), ("B", 2), ("C", 3)])
E = Enum("E", (("A", 1), ("B", 2), ("C", 3)))
E = Enum("E", {"A": 1, "B": 2, "C": 3})

E = Enum("E", [])
E = Enum("E", ())
E = Enum("E", {})


### No errors

E = Enum("E")

E = Enum("E", "A-B, C")

E = Enum("E", {"A", "B", "C"})

E = Enum("E", ["A", ("B", 2), "C"])
E = Enum("E", ["A", *a])
E = Enum("E", {"A": 1, **a})

E = Enum("E", {
    "A": 1,  # Comment
})
