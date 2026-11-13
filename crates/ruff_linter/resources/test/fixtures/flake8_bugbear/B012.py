def a():
    try:
        pass
    finally:
        return  # warning


def b():
    try:
        pass
    finally:
        if 1 + 0 == 2 - 1:
            return  # warning


def c():
    try:
        pass
    finally:
        try:
            return  # warning
        except Exception:
            pass


def d():
    try:
        try:
            pass
        finally:
            return  # warning
    finally:
        pass


def e():
    if 1 == 2 - 1:
        try:

            def f():
                try:
                    pass
                finally:
                    return  # warning

        finally:
            pass


def g():
    try:
        pass
    finally:

        def h():
            return  # no warning

        e()


def i():
    while True:
        try:
            pass
        finally:
            break  # warning

            def j():
                while True:
                    break  # no warning


def h():
    while True:
        try:
            pass
        finally:
            continue  # warning

            def j():
                while True:
                    continue  # no warning


def k():
    try:
        pass
    finally:
        while True:
            break  # no warning
        while True:
            continue  # no warning
        while True:
            return  # warning


while True:
    try:
        pass
    finally:
        continue  # warning

while True:
    try:
        pass
    finally:
        break  # warning


while True:
    try:
        pass
    finally:
        match *0, 1, *2:
            case 0,:
                y = 0
            case 0, *x:
                break  # warning


while True:
    try:
        pass
    finally:
        match *0, 1, *2:
            case 0,:
                y = 0
            case 0, *x:
                pass  # no warning


# Regression: return inside an if-else branch.
def regression_if_else(condition):
    try:
        pass
    finally:
        if condition:
            pass
        else:
            return 1  # warning


# Regression: return inside an except handler.
def regression_except():
    try:
        pass
    finally:
        try:
            pass
        except ValueError:
            return 2  # warning


# Regression: ordinary return outside finally.
def regression_no_warning():
    if True:
        return 3  # no warning


def regression_loop_else():
    try:
        pass
    finally:
        for item in []:
            pass
        else:
            return 6  # warning