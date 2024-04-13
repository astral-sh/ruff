# Regression test for: https://github.com/astral-sh/ruff/issues/7370
result = (
    f(111111111111111111111111111111111111111111111111111111111111111111111111111111111)
    + 1
)[0]

# Regression tests for: https://github.com/astral-sh/ruff/issues/10355
repro(
    "some long string that takes up some space"
)[  # some long comment also taking up space
    0
]

repro(
    "some long string that takes up some space"
)[0  # some long comment also taking up space
]

repro(
    "some long string that takes up some space"
)[0]  # some long comment also taking up space

repro("some long string that takes up some space")[0]  # some long comment also taking up space

repro(
    "some long string that takes up some space"
)[  # some long comment also taking up space
0:-1
]

(
    repro
)[  # some long comment also taking up space
    0
]

(
    repro  # some long comment also taking up space
)[
    0
]
