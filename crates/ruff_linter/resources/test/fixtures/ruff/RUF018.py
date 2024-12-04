# RUF018
assert (x := 0) == 0
assert x, (y := "error")
print(x, y)

# OK
if z := 0:
    pass


# These should not be flagged, because the only uses of the variables defined
# are themselves within `assert` statements.

# Here the `t` variable is referenced
# from a later `assert` statement:
assert (t:=cancel((F, G))) == (1, P, Q)
assert isinstance(t, tuple)

# Here the `g` variable is referenced from within the same `assert` statement:
assert (g:=solve(groebner(eqs, s), dict=True)) == sol, g
