### Errors (both RUF047 and RUF050 converge)

# RUF047 removes the else, then RUF050 removes the remaining if.
if sys.version_info >= (3, 11):
    pass
else:
    pass

# Same with elif.
if sys.version_info >= (3, 11):
    pass
elif sys.version_info >= (3, 10):
    pass
else:
    pass

# Side-effect in condition: RUF047 removes the else, then RUF050
# replaces the remaining `if` with the condition expression
if foo():
    pass
else:
    pass


### No errors

# Non-empty if body: neither rule fires.
if sys.version_info >= (3, 11):
    bar()
else:
    baz()
