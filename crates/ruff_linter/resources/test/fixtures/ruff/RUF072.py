# Errors

# Case A: try/except/finally with pass
try:
    foo()
except Exception:
    bar()
finally:
    pass

# Case A: try/except/finally with ellipsis
try:
    foo()
except Exception:
    bar()
finally:
    ...

# Case A: try/except/else/finally with pass
try:
    foo()
except Exception:
    bar()
else:
    baz()
finally:
    pass

# Case B: bare try/finally with pass
try:
    foo()
finally:
    pass

# Case B: bare try/finally with ellipsis
try:
    foo()
finally:
    ...

# Case B: bare try/finally with multi-line body
try:
    foo()
    bar()
    baz()
finally:
    pass

# Nested try with useless finally
try:
    try:
        foo()
    finally:
        pass
except Exception:
    bar()

# OK

# finally with real code
try:
    foo()
finally:
    cleanup()

# finally with pass and other statements
try:
    foo()
finally:
    pass
    cleanup()

# No finally at all
try:
    foo()
except Exception:
    bar()

# Comments — diagnostic but no fix

# Comment on `finally:` line
try:
    foo()
except Exception:
    bar()
finally:  # comment
    pass

# Comment on `pass` line
try:
    foo()
except Exception:
    bar()
finally:
    pass  # comment

# Preceding own-line comment
try:
    foo()
except Exception:
    bar()
# comment
finally:
    pass

# Trailing own-line comment
try:
    foo()
except Exception:
    bar()
finally:
    pass
    # comment

# Own-line comment before `pass` in the finally body
try:
    foo()
except Exception:
    bar()
finally:
    # comment
    pass

# Comment on bare try/finally
try:
    foo()
finally:  # comment
    pass
