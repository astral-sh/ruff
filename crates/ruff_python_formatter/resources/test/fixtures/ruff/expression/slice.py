# Handle comments both when lower and upper exist and when they don't
a1 = "a"[
    # a
    1  # b
    :  # c
    2  # d
]
a2 = "a"[
    # a
      # b
    :  # c
      # d
]

# Check all places where comments can exist
b1 = "b"[ # a
    # b
    1 # c
    # d
    : # e
    # f
    2 # g
    # h
    : # i
    # j
    3 # k
    # l
]

# Handle the spacing from the colon correctly with upper leading comments
c1 = "c"[
    1
    :  # e
    # f
    2
]
c2 = "c"[
    1
    :  # e
    2
]
c3 = "c"[
    1
    :
    # f
    2
]
c4 = "c"[
    1
    : # f
    2
]

# End of line comments
d1 = "d"[ # comment
    :
]
d2 = "d"[  # comment
    1:
]
d3 = "d"[
    1  # comment
    :
]

# Spacing around the colon(s)
def a():
    pass

e00 = "e"[:]
e01 = "e"[:1]
e02 = "e"[: a()]
e03 = "e"[:-1]
e10 = "e"[1:]
e11 = "e"[1:1]
e12 = "e"[1 : a()]
e13 = "e"[1:-1]
e20 = "e"[a() :]
e21 = "e"[a() : 1]
e22 = "e"[a() : a()]
e23 = "e"[a() : -1]
e200 = "e"[a() :: ]
e201 = "e"[a() :: 1]
e202 = "e"[a() :: a()]
e210 = "e"[a() : 1 :]

# Regression test for https://github.com/astral-sh/ruff/issues/5605
f = "f"[:,]

# Regression test for https://github.com/astral-sh/ruff/issues/5733
g1 = "g"[(1):(2)]
g2 = "g"[(1):(2):(3)]

# Don't omit optional parentheses for subscripts
# https://github.com/astral-sh/ruff/issues/7319
def f():
    return (
        package_version is not None
        and package_version.split(".")[:2] == package_info.version.split(".")[:2]
    )


# Group to ensure other arguments don't expand.
self.assertEqual(
    houses.all()[0].occupants.all()[0].houses.all()[1].rooms.all()[0],
    self.room2_1,
)

self.assertEqual(
    suite._tests[0].id().split(".")[0],
    os.path.basename(os.getcwd()),
)

# PEP 646 introduced starred expression in indexes
# https://peps.python.org/pep-0646/#change-1-star-expressions-in-indexes
data[*x]
data[*x,]
data[
    *x,
]
data[  # comment 1
     *x,  # comment 2
]  # comment 3
