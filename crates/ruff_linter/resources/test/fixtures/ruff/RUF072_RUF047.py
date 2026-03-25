# RUF072 removes the empty `finally`, RUF047 removes the empty `else`
# Both fixes apply independently on the same try statement
try:
    foo()
except Exception:
    bar()
else:
    pass
finally:
    pass

# All non-body clauses are no-ops
try:
    foo()
except Exception:
    pass
else:
    pass
finally:
    pass

# Only the `finally` is empty; `else` has real code
try:
    foo()
except Exception:
    bar()
else:
    baz()
finally:
    pass
