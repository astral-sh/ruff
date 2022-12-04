def f():
    try:
        raise ValueError('ve')
    except ValueError as exc:
        pass

    print("Last exception:", exc)

def f():
    exc = 1

    try:
        raise ValueError('ve')
    except ValueError as exc:
        pass

    print("Last exception:", exc)
