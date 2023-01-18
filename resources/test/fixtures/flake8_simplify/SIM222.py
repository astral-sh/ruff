if a or True:  # SIM223
    pass

if (a or b) or True:  # SIM223
    pass

if a or (b or True):  # SIM223
    pass

if a and True:  # OK
    pass

if True:  # OK
    pass


def validate(self, value):
    return json.loads(value) or True  # OK
