a = True if b else False  # SIM210

a = True if b != c else False  # SIM210

a = True if b + c else False  # SIM210

a = False if b else True  # OK

def f():
    # OK
    def bool():
        return False

    a = True if b else False
