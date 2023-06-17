x = 1
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa = 1
b, c, d = (2, 3, 4)

# Some comment
del x  # Trailing comment
# Dangling comment

# Some comment
del x, aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa, b, c, d  # Trailing comment
# Dangling comment

# Some comment
del (
    x,
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,
    b,
    c,
    d
) # Trailing comment
# Dangling comment
