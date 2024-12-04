###
# E = Empty
# N = Not empty
# O = Omitted
###


# Unsafe fix: Remove statement
for E in O:
    ...


# Unsafe fix: Remove statement
for E in E:
    pass
else:
    ...


# No fix
for E in N:
    pass
else:
    print()


# No error
for N in O:
    print()


# Safe fix: Remove `else` branch
for N in E:
    print()
else:
    ...


# No error
for N in N:
    print()
else:
    print()
