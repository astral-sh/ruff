if not a == b:  # SIM201
    pass

if not a == (b + c):  # SIM201
    pass

if not (a + b) == c:  # SIM201
    pass

if not a != b:  # OK
    pass

if a == b:  # OK
    pass

if not a == b:  # OK
    raise ValueError()
