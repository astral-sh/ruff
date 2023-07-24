# Leading
lambda x: x  # Trailing
# Trailing

# Leading
lambda x, y: x  # Trailing
# Trailing

# Leading
lambda x, y: x, y  # Trailing
# Trailing

# Leading
lambda x, /, y: x  # Trailing
# Trailing

# Leading
lambda x: lambda y: lambda z: x  # Trailing
# Trailing

# Leading
lambda x: lambda y: lambda z: (x, y, z)  # Trailing
# Trailing

# Leading
lambda x: lambda y: lambda z: (
    x,
    y,
z)  # Trailing
# Trailing

# Leading
lambda x: lambda y: lambda z: (
    x,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
    y,
z)  # Trailing
# Trailing


a = (
    lambda  # Dangling
           : 1
)

# Regression test: lambda empty arguments ranges were too long, leading to unstable
# formatting
(lambda:(#
),)
