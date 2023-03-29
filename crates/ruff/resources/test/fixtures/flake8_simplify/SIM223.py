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

if a and f() and b and g() and False:  # OK
    pass

if a and f() and False and g() and b:  # SIM222
    pass

if False and f() and a and g() and b:  # SIM222
    pass

if a and False and f() and b and g():  # SIM222
    pass
