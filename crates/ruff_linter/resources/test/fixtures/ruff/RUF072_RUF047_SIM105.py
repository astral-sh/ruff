# All three non-body clauses are no-ops — after all rules converge,
# only `contextlib.suppress(Exception): foo()` remains
try:
    foo()
except Exception:
    pass
else:
    pass
finally:
    pass
