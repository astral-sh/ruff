def zipped():
    return zip([1, 2, 3], "ABC")

# Errors.

# FURB140
[print(x, y) for x, y in zipped()]

# FURB140
(print(x, y) for x, y in zipped())

# FURB140
{print(x, y) for x, y in zipped()}


from itertools import starmap as sm

# FURB140
[print(x, y) for x, y in zipped()]

# FURB140
(print(x, y) for x, y in zipped())

# FURB140
{print(x, y) for x, y in zipped()}

# Non-errors.

[print(x, int) for x, _ in zipped()]

[print(x, y, 1) for x, y in zipped()]

[print(y, x) for x, y in zipped()]

[print(x + 1, y) for x, y in zipped()]

[print(x) for x in range(100)]

[print() for x, y in zipped()]

[print(x, end=y) for x, y in zipped()]
