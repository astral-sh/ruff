class A:
    pass


def f() -> "A":
    pass


def g() -> "///":
    pass


X: """List[int]"""'☃' = []

# Type annotations with triple quotes can contain newlines and indentation
# https://github.com/python/typing-council/issues/9
y: """

   int |
   str
"""

z: """(

    int |
    str
)
"""

invalid1: """
		int |
str)
"""

invalid2: """
		int) |
str
"""
invalid3: """
		((int)
"""
invalid4: """
		(int
"""

# Single-quoted annotations are parsed the same way, so leading whitespace is allowed
# https://github.com/astral-sh/ty/issues/4529
w: "  int"
