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

class Foo:
    def value(self):
        return 1

def unwrap(value):
    if isinstance(value, Foo):
        foo = value
        return foo.value()
    elif type(value) is tuple:
        length = len(value)
        if length == 0:
            return ()
        elif length == 1:
            return (unwrap(value[0]),)
        else:
            result = []
            for item in value:
                result.append(unwrap(item))
            return tuple(result)
    else:
        raise TypeError()
