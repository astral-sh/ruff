# We should not treat the trailing comma
# in a single-element subscript.
a: tuple[int,]
b = tuple[int,]

# The magic comma still applies to multi-element subscripts.
c: tuple[
    int,
    int,
]
d = tuple[
    int,
    int,
]

# Magic commas still work as expected for non-subscripts.
small_list = [
    1,
]
list_of_types = [
    tuple[int,],
]
