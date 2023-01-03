if a and False:  # SIM223
    pass

if (a or b) and False:  # SIM223
    pass

if a or (b and False):  # SIM223
    pass

if a or False:
    pass

if False:
    pass
