def f(cond: bool):
    if cond:
        result = ()
        result += (f(cond),)
        return result

    return None

reveal_type(f(True))

def f(cond: bool):
    if cond:
        result = ()
        result += (f(cond),)
        return result

    return None

def f(cond: bool):
    result = None
    if cond:
        result = ()
        result += (f(cond),)

    return result

reveal_type(f(True))

def f(cond: bool):
    result = None
    if cond:
        result = [f(cond) for _ in range(1)]

    return result

reveal_type(f(True))