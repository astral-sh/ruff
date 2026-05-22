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

# SIM109 (string literals)
if a == "foo" or a == "bar":
    d

# SIM109 (integer literals)
if a == 1 or a == 2:
    d

# SIM109 (mixed: variable and literal)
if a == b or a == "baz":
    d
