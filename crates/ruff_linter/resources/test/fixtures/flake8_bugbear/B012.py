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


def test_nested_if_else(a):
    try:
        pass
    finally:
        if a:
            pass
        else:
            return 1  # warning


def test_nested_elif(a, b):
    try:
        pass
    finally:
        if a:
            pass
        elif b:
            return 2  # warning
        else:
            pass


def test_nested_try_except():
    try:
        pass
    finally:
        try:
            pass
        except ValueError:
            return 3  # warning


def test_nested_try_else():
    try:
        pass
    finally:
        try:
            pass
        except ValueError:
            pass
        else:
            return 4  # warning


def test_nested_for_else(xs):
    try:
        pass
    finally:
        for x in xs:
            pass
        else:
            return 5  # warning


def test_nested_while_else():
    try:
        pass
    finally:
        while False:
            pass
        else:
            return 6  # warning


def test_nested_for_else_break(xs):
    while True:
        try:
            pass
        finally:
            for x in xs:
                pass
            else:
                break  # warning


def test_loop_break_in_finally_else():
    try:
        pass
    finally:
        while True:
            for x in []:
                pass
            else:
                break  # no warning (breaks inner while)


def test_loop_return_in_finally_else():
    try:
        pass
    finally:
        while True:
            for x in []:
                pass
            else:
                return 7  # warning

