x = 1  # noqa
x = 1  # NOQA:F401,W203
# noqa
# NOQA
# noqa:F401
# noqa:F401,W203

x = 1
x = 1  # noqa: F401, W203
# noqa: F401
# noqa: F401, W203

# OK
x = 2  # noqa: X100
x = 2  # noqa:X100

# PGH004
x = 2  # noqa X100

# PGH004
x = 2  # noqa X100, X200

# PGH004
x = 2  # noqa : X300

# PGH004
x = 2  # noqa  : X400

# PGH004
x = 2  # noqa :X500
