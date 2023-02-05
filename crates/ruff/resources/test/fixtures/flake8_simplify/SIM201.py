# SIM201
if not a == b:
    pass

# SIM201
if not a == (b + c):
    pass

# SIM201
if not (a + b) == c:
    pass

# OK
if not a != b:
    pass

# OK
if a == b:
    pass

# OK
if not a == b:
    raise ValueError()

# OK
def __ne__(self, other):
    return not self == other
