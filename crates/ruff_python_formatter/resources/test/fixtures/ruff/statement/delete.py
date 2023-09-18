x = 1
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa = 1
a, b, c, d = 1, 2, 3, 4

del a, b, c, d
del a, b, c, d  # Trailing

del a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a
del a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a, a  # Trailing

del (
    a,
    a
)

del (
    # Dangling comment
)

# Delete something
del x  # Deleted something
# Done deleting

# Delete something
del (
    # Deleting something
    x  # Deleted something
    # Finishing deletes
)  # Completed
# Done deleting

# Delete something
del (
    # Deleting something
    x  # Deleted something
    # Finishing deletes
)  # Completed
# Done deleting

# Delete something
del (
    # Deleting something
    x,  # Deleted something
    # Finishing deletes
)  # Completed
# Done deleting

# Delete something
del (
    # Deleting something
    x  # Deleted something
    # Finishing deletes
    # Dangling comment
)  # Completed
# Done deleting

# NOTE: This shouldn't format. See https://github.com/astral-sh/ruff/issues/5630.
# Delete something
del x, aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa, b, c, d  # Delete these
# Ready to delete

# Delete something
del (
    x,
    # Deleting this
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
    b,
    c,
    d,
    # Deleted
)  # Completed
# Done

del (  # dangling end of line comment
)

del (  # dangling end of line comment
    # dangling own line comment
)  # trailing statement comment
