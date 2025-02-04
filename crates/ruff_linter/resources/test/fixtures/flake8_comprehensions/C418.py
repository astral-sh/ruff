dict({})
dict({'a': 1})
dict({'x': 1 for x in range(10)})
dict(
    {'x': 1 for x in range(10)}
)

dict({}, a=1)
dict({x: 1 for x in range(1)}, a=1)

# Skip when too many positional arguments
# See https://github.com/astral-sh/ruff/issues/15810
dict({"A": 1}, {"B": 2})
