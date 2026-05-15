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

# Regression test for: https://github.com/astral-sh/ruff/issues/19601
# The fix must preserve the target's source verbatim — re-rendering through
# the AST mangles f-strings whose format spec contains escape sequences or
# whose interpolations include lambdas.
isinstance(f"{(lambda: 0)}", int) or isinstance(f"{(lambda: 0)}", str)
isinstance(f"{0:{(lambda: 0)}}", int) or isinstance(f"{0:{(lambda: 0)}}", str)
isinstance(f"{0:\x22}", int) or isinstance(f"{0:\x22}", str)
isinstance(f"{0:\x7b}", int) or isinstance(f"{0:\x7b}", str)

# Regression test for: https://github.com/astral-sh/ruff/pull/25061
types = (int,)
isinstance(x, (*types,)) or isinstance(x, ())
isinstance(x, (*types,)) or isinstance(x, (*types,))
isinstance(x, ()) or isinstance(x, int)
isinstance(x, ()) or isinstance(x, ())
((isinstance(x, int)) or isinstance(x, str))
isinstance(x, int) or (isinstance(x, str))
