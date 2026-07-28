x = {
    "a": 1,
    "a": 2,
    "b": 3,
    ("a", "b"): 3,
    ("a", "b"): 4,
    1.0: 2,
    1: 0,
    1: 3,
    b"123": 1,
    b"123": 4,
}

x = {
    "a": 1,
    "a": 2,
    "a": 3,
    "a": 3,
}

x = {
    "a": 1,
    "a": 2,
    "a": 3,
    "a": 3,
    "a": 4,
}

x = {
    "a": 1,
    "a": 1,
    "a": 2,
    "a": 3,
    "a": 4,
}

x = {
    a: 1,
    "a": 1,
    a: 1,
    "a": 2,
    a: 2,
    "a": 3,
    a: 3,
    "a": 3,
    a: 4,
}

x = {"a": 1, "a": 1}
x = {"a": 1, "b": 2, "a": 1}

x = {
    ('a', 'b'): 'asdf',
    ('a', 'b'): 'qwer',
}

# Regression test for: https://github.com/astral-sh/ruff/issues/4897
t={"x":"test123", "x":("test123")}

t={"x":("test123"), "x":"test123"}

# Regression test for: https://github.com/astral-sh/ruff/issues/12772
x = {
    1: "abc",
    1: "def",
    True: "ghi",
    0: "foo",
    0: "bar",
    False: "baz",
}

x = {(f(), 2): 1, (f(), 2.0): 2}
x = {9007199254740993: 1, 9007199254740993 + 0j: 2}

# Regression test for: https://github.com/astral-sh/ruff/pull/25982#pullrequestreview-4495269928
x = {False: 1, -0: 2}
x = {-0: 1, False: 2}
x = {True: 1, 1 + 0j: 2}
x = {1 + 0j: 1, True: 2}
x = {-1: 1, -1 + 0j: 2}
x = {-1 + 0j: 1, -1: 2}
x = {-1 + 2j: 1, -1 + 2j: 2}
