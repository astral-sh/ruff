x = 1
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa = 1
b, c, d = (2, 3, 4)

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
) # Completed
# Done deleting

# Delete something
del (
    # Deleting something
    x  # Deleted something
    # Finishing deletes
) # Completed
# Done deleting

# Delete something
del (
    # Deleting something
    x,  # Deleted something
    # Finishing deletes
) # Completed
# Done deleting

# Delete something
del (
    # Deleting something
    x  # Deleted something
    # Finishing deletes

    # Dangling comment
) # Completed
# Done deleting

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
    d
    # Deleted
) # Completed
# Done
