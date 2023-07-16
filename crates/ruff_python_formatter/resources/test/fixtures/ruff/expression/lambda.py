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

