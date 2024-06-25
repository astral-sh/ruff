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

if isinstance(a.b, int) or isinstance(a.b, float):  # SIM101
    pass

if isinstance(a(), int) or isinstance(a(), float):  # SIM101
    pass

if isinstance(a, int) and isinstance(b, bool) or isinstance(a, float):
    pass

if isinstance(a, bool) or isinstance(b, str):
    pass

if isinstance(a, int) or isinstance(a.b, float):
    pass

# OK
if isinstance(a, int) or unrelated_condition or isinstance(a, float):
    pass

if x or isinstance(a, int) or isinstance(a, float):
    pass

if x or y or isinstance(a, int) or isinstance(a, float) or z:
    pass

def f():
    # OK
    def isinstance(a, b):
        return False

    if isinstance(a, int) or isinstance(a, float):
        pass

# Regression test for: https://github.com/astral-sh/ruff/issues/7455#issuecomment-1722460483
if(isinstance(a, int)) or (isinstance(a, float)):
    pass
