###
# E = Empty
# N = Not empty
# O = Omitted
###


# Unsafe fix: Remove statement
if EOO:
    pass


# Unsafe fix: Remove statement
if EOE:
    ...
else:
    pass


# No fix
if EON:
    ...
else:
    print()


# Unsafe fix: Remove statement
if EEO:
    pass
elif _:
    ...


# Unsafe fix: Remove statement
if EEE:
    pass
elif _:
    ...
else:
    pass


# No fix
# Display-only fix: Remove `elif` branch
if EEN:
    pass
elif _:
    ...
else:
    print()


# No fix
if ENO:
    pass
elif _:
    print()


# No fix
# Safe fix: Remove `else` branch
if ENE:
    ...
elif _:
    print()
else:
    pass


# No fix
if ENN:
    ...
elif _:
    print()
else:
    print()


# No error
if NOO:
    print()


# Safe fix: Remove `else` branch
if NOE:
    print()
else:
    pass


# No error
if NON:
    print()
else:
    print()


# Unsafe fix: Remove `elif` branch
if NEO:
    print()
elif _:
    ...


# Display-only fix: Remove `elif` branch
# Safe fix: Remove `else` branch
if NEE:
    print()
elif _:
    pass
else:
    ...


# Display-only fix: Remove `elif` branch
if NEN:
    print()
elif _:
    pass
else:
    print()


# No error
if NNO:
    print()
elif _:
    print()


# Safe fix: Remove `else` branch
if NNE:
    print()
elif _:
    print()
else:
    ...


# No error
if NNN:
    print()
elif _:
    print()
else:
    print()


#####


# Display-only fix: Remove `elif` branch
# Unsafe fix: Remove `elif` branch
if NEE_:
    print()
elif _:
    pass
elif _:
    ...


# Unsafe fix: Remove `elif` branch
if NEO_:
    print()
# Lorem ipsum
elif _:
    ...


# Unsafe fix: Remove `else` branch
if NOE_:
    print()
else:
    # Lorem ipsum
    pass
