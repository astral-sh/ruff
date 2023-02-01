if isinstance(a, int) or isinstance(a, float):  # SIM101
    pass

if isinstance(a, (int, float)) or isinstance(a, bool):  # SIM101
    pass

if isinstance(a, int) or isinstance(a, float) or isinstance(b, bool):  # SIM101
    pass

if isinstance(b, bool) or isinstance(a, int) or isinstance(a, float):  # SIM101
    pass

if isinstance(a, int) or isinstance(b, bool) or isinstance(a, float):  # SIM101
    pass

if (isinstance(a, int) or isinstance(a, float)) and isinstance(b, bool):  # SIM101
    pass

if isinstance(a, int) and isinstance(b, bool) or isinstance(a, float):
    pass

if isinstance(a, bool) or isinstance(b, str):
    pass

def f():
    # OK
    def isinstance(a, b):
        return False
    if isinstance(a, int) or isinstance(a, float):
        pass
