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

# TODO: If this is commented out, that is, if `infer_scope_types` is called before `infer_return_type`, it will panic.
reveal_type(unwrap(Foo()))

def descent(x: int, y: int):
    if x > y:
        y, x = descent(y, x)
        return x, y
    if x == 1:
        return (1, 0)
    if y == 1:
        return (0, 1)
    else:
        return descent(x-1, y-1)

# TODO: If this is commented out, that is, if `infer_scope_types` is called before `infer_return_type`, it will panic.
reveal_type(descent(5, 3))
