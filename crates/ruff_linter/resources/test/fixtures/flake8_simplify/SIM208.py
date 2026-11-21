if not (not a):  # SIM208
    pass

if not (not (a == b)):  # SIM208
    pass

if not a:  # OK
    pass

if not a == b:  # OK
    pass

if not a != b:  # OK
    pass

a = not not b  # SIM208

f(not not a)  # SIM208

if 1 + (not (not a)):  # SIM208
    pass

# https://github.com/astral-sh/ruff/issues/30414
print(not not x in y)  # SIM208
print(not not x not in y)  # SIM208
print(not not x is y)  # SIM208
print(not not x is not y)  # SIM208
print(not not bool(x))  # SIM208
print(not not x in y in z)  # SIM208

# These may not be `bool`, so the fix still wraps them in `bool(...)`.
print(not not x == y)  # SIM208
print(not not x == y in z)  # SIM208


def shadowed_bool(bool):
    # No fix: the operand needs a `bool(...)` wrapper, but `bool` is shadowed.
    print(not not x)  # SIM208
    # No fix: this `bool(...)` call is not the builtin, so it may not return a `bool`.
    print(not not bool(x))  # SIM208
    # Fixed: the operand is always a `bool`, so no wrapper is needed.
    print(not not x in y)  # SIM208
