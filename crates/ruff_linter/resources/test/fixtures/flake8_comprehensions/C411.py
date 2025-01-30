x = [1, 2, 3]
list([i for i in x])

# Skip when too many positional arguments
# or keyword argument present.
# See https://github.com/astral-sh/ruff/issues/15810
list([x for x in "XYZ"],[])
list([x for x in "XYZ"],foo=[])
