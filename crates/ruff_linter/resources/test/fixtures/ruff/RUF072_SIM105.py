# SIM105 cannot fire while `finally` is non-empty
# RUF072 removes the empty `finally` first, then SIM105 rewrites
# the `try/except: pass` to `contextlib.suppress()` on the next pass
try:
    foo()
except Exception:
    pass
finally:
    pass

