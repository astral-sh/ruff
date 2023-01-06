if not a != b:  # SIM202
    pass

if not a != (b + c):  # SIM202
    pass

if not (a + b) != c:  # SIM202
    pass

if not a == b:  # OK
    pass

if a != b:  # OK
    pass
