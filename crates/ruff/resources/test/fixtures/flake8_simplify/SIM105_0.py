def foo():
    pass


# SIM105
try:
    foo()
except ValueError:
    pass

# SIM105
try:
    foo()
except (ValueError, OSError):
    pass

# SIM105
try:
    foo()
except:
    pass

# SIM105
try:
    foo()
except (a.Error, b.Error):
    pass

# OK
try:
    foo()
except ValueError:
    print("foo")
except OSError:
    pass

# OK
try:
    foo()
except ValueError:
    pass
else:
    print("bar")

# OK
try:
    foo()
except ValueError:
    pass
finally:
    print("bar")

# OK
try:
    foo()
    foo()
except ValueError:
    pass

# OK
try:
    for i in range(3):
        foo()
except ValueError:
    pass


def bar():
    # OK
    try:
        return foo()
    except ValueError:
        pass


def with_ellipsis():
    # OK
    try:
        foo()
    except ValueError:
        ...


def with_ellipsis_and_return():
    # OK
    try:
        return foo()
    except ValueError:
        ...


def with_comment():
    try:
        foo()
    except (ValueError, OSError):
        pass  # Trailing comment.
