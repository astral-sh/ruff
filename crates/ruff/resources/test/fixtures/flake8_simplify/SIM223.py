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

if a and f() and False and g() and b:  # SIM223
    pass

if False and f() and a and g() and b:  # SIM223
    pass

if a and False and f() and b and g():  # SIM223
    pass


if a or f() or b or g() or True:  # OK
    pass

if a or f() or True or g() or b:  # OK
    pass

if True or f() or a or g() or b:  # OK
    pass

if a or True or f() or b or g():  # OK
    pass
