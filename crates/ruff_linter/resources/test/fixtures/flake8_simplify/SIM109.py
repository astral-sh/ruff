# SIM109
if a == b or a == c:
    d

# SIM109
if (a == b or a == c) and None:
    d

# SIM109
if a == b or a == c or None:
    d

# SIM109
if a == b or None or a == c:
    d

# OK
if a in (b, c):
    d

# OK
if a == b or a == c():
    d

# OK
if (
    a == b
    # This comment prevents us from raising SIM109
    or a == c
):
    d
