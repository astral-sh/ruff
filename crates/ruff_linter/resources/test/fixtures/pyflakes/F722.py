class A:
    pass


def f() -> "A":
    pass


def g() -> "///":
    pass


X: """List[int]"""'☃' = []

y: """

   int |
   str
"""

z: """(

    int |
    str
)
"""

# single quotes are not implicitly parenthesized
invalid: "\n int"


invalid2: """
		int |
str)
"""

invalid3: """
		int) |
str
"""
