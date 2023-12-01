# SIM202
if not a != b:
    pass

# SIM202
if not a != (b + c):
    pass

# SIM202
if not (a + b) != c:
    pass

# OK
if not a == b:
    pass

# OK
if a != b:
    pass

# OK
if not a != b:
    raise ValueError()

# OK
def __eq__(self, other):
    return not self != other
