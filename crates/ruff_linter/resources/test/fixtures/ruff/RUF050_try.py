###
# E = Empty
# N = Not empty
# O = Omitted
###


# Safe fix: Remove statement
try:
    pass
except:
    ...


# Safe fix: Remove statement
try:
    pass
except:
    ...
finally:
    pass


# No fix
try:
    ...
except:
    pass
finally:
    print()


# Safe fix: Remove statement
try:
    ...
except:
    pass
else:
    ...


# Safe fix: Remove statement
try:
    pass
except:
    ...
else:
    pass
finally:
    ...


# No fix
# Safe fix: Remove `else` branch
try:
    pass
except:
    ...
else:
    pass
finally:
    print()


# No fix
try:
    ...
except:
    pass
else:
    print()


# No fix
# Safe fix: Remove `finally` branch
try:
    ...
except:
    pass
else:
    print()
finally:
    ...


# No fix
try:
    pass
except:
    ...
else:
    print()
finally:
    print()


# Safe fix: Remove statement
try:
    pass
except:
    print()


# Safe fix: Remove statement
try:
    ...
except:
    print()
finally:
    pass


# No fix
try:
    ...
except:
    print()
finally:
    print()


# Safe fix: Remove statement
try:
    pass
except:
    print()
else:
    ...


# Safe fix: Remove statement
try:
    pass
except:
    print()
else:
    ...
finally:
    pass


# No fix
# Safe fix: Remove `else` branch
try:
    ...
except:
    print()
else:
    pass
finally:
    print()


# No fix
try:
    ...
except:
    print()
else:
    print()


# No fix
# Safe fix: Remove `finally` branch
try:
    pass
except:
    print()
else:
    print()
finally:
    ...


# No fix
try:
    pass
except:
    print()
else:
    print()
finally:
    print()


# Safe fix: Remove `finally` branch
try:
    print()
except:
    ...


# Safe fix: Remove `finally` branch
try:
    print()
except:
    pass
finally:
    ...


# No error
try:
    print()
except:
    pass
finally:
    print()


# Safe fix: Remove `else` branch
try:
    print()
except:
    ...
else:
    pass


# Safe fix: Remove `else` branch
# Safe fix: Remove `finally` branch
try:
    print()
except:
    ...
else:
    pass
finally:
    ...


# Safe fix: Remove `else` branch
try:
    print()
except:
    pass
else:
    ...
finally:
    print()


# No error
try:
    print()
except:
    pass
else:
    print()


# Safe fix: Remove `finally` branch
try:
    print()
except:
    ...
else:
    print()
finally:
    pass


# No error
try:
    print()
except:
    ...
else:
    print()
finally:
    print()


# No error
try:
    print()
except:
    print()


# Safe fix: Remove `finally` branch
try:
    print()
except:
    print()
finally:
    pass


# No error
try:
    print()
except:
    print()
finally:
    print()


# Safe fix: Remove `else` branch
try:
    print()
except:
    print()
else:
    ...


# Safe fix: Remove `else` branch
# Safe fix: Remove `finally` branch
try:
    print()
except:
    print()
else:
    pass
finally:
    ...


# Safe fix: Remove `else` branch
try:
    print()
except:
    print()
else:
    pass
finally:
    print()


# No error
try:
    print()
except:
    print()
else:
    print()


# Safe fix: Remove `finally` branch
try:
    print()
except:
    print()
else:
    print()
finally:
    ...


# No error
try:
    print()
except:
    print()
else:
    print()
finally:
    print()
