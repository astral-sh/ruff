if a or True:  # SIM222
    pass

if (a or b) or True:  # SIM222
    pass

if a or (b or True):  # SIM222
    pass

if a and True:  # OK
    pass

if True:  # OK
    pass


def validate(self, value):
    return json.loads(value) or True  # OK


if a or f() or b or g() or True:  # OK
    pass

if a or f() or True or g() or b:  # SIM222
    pass

if True or f() or a or g() or b:  # SIM222
    pass

if a or True or f() or b or g():  # SIM222
    pass


if a and f() and b and g() and False:  # OK
    pass

if a and f() and False and g() and b:  # OK
    pass

if False and f() and a and g() and b:  # OK
    pass

if a and False and f() and b and g():  # OK
    pass
