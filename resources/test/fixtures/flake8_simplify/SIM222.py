if a or True:  # SIM223
    pass

if (a or b) or True:  # SIM223
    pass

if a or (b or True):  # SIM223
    pass

if a and True:
    pass

if True:
    pass
