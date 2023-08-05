# Errors
a = "hello"

# SIM116
if a == "foo":
    return "bar"
elif a == "bar":
    return "baz"
elif a == "boo":
    return "ooh"
else:
    return 42

# SIM116
if a == 1:
    return (1, 2, 3)
elif a == 2:
    return (4, 5, 6)
elif a == 3:
    return (7, 8, 9)
else:
    return (10, 11, 12)

# SIM116
if a == 1:
    return (1, 2, 3)
elif a == 2:
    return (4, 5, 6)
elif a == 3:
    return (7, 8, 9)

# SIM116
if a == "hello 'sir'":
    return (1, 2, 3)
elif a == 'goodbye "mam"':
    return (4, 5, 6)
elif a == """Fairwell 'mister'""":
    return (7, 8, 9)
else:
    return (10, 11, 12)

# SIM116
if a == b"one":
    return 1
elif a == b"two":
    return 2
elif a == b"three":
    return 3

# SIM116
if a == "hello 'sir'":
    return ("hello'", 'hi"', 3)
elif a == 'goodbye "mam"':
    return (4, 5, 6)
elif a == """Fairwell 'mister'""":
    return (7, 8, 9)
else:
    return (10, 11, 12)

# OK
if a == "foo":
    return "bar"
elif a == "bar":
    return baz()
elif a == "boo":
    return "ooh"
else:
    return 42

# OK
if a == b"one":
    return 1
elif b == b"two":
    return 2
elif a == b"three":
    return 3

# SIM116
if func_name == "create":
    return "A"
elif func_name == "modify":
    return "M"
elif func_name == "remove":
    return "D"
elif func_name == "move":
    return "MV"

# OK
def no_return_in_else(platform):
    if platform == "linux":
        return "auditwheel repair -w {dest_dir} {wheel}"
    elif platform == "macos":
        return "delocate-wheel --require-archs {delocate_archs} -w {dest_dir} -v {wheel}"
    elif platform == "windows":
        return ""
    else:
        msg = f"Unknown platform: {platform!r}"
        raise ValueError(msg)
