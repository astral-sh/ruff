ápple_count: int = 1  # C2401
ápple_count += 2  # C2401
ápple_count = 3  # C2401

# this rule only works on assignment!
ápple_count == 3  # Ok

# normal ascii
apple_count = 4  # Ok
