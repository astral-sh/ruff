# Errors

# try/except/finally with pass
try:
    foo()
except Exception:
    bar()
finally:
    pass

# try/except/finally with ellipsis
try:
    foo()
except Exception:
    bar()
finally:
    ...

# try/except/else/finally with pass
try:
    foo()
except Exception:
    bar()
else:
    baz()
finally:
    pass

# bare try/finally with pass
try:
    foo()
finally:
    pass

# bare try/finally with ellipsis
try:
    foo()
finally:
    ...

# bare try/finally with multi-line body
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

# finally with two pass statements
try:
    foo()
except Exception:
    bar()
finally:
    pass
    pass

# bare try/finally with pass and ellipsis
try:
    foo()
finally:
    pass
    ...

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

# Own-line comment extra-indented before `pass`
try:
    foo()
except Exception:
    bar()
finally:
        # comment
    pass

# Trailing comment indented one level (belongs to finally body)
try:
    foo()
except Exception:
    bar()
finally:
    pass
        # indented comment

# Trailing comment dedented one level (not part of finally, but
# immediately adjacent — suppresses fix conservatively)
try:
    foo()
except Exception:
    bar()
finally:
    pass
# dedented comment

# Comment on bare try/finally
try:
    foo()
finally:  # comment
    pass
