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

def count_set_bits(n):
    return 1 + count_set_bits(n & n - 1) if n else 0

class Literal:
    def __invert__(self):
        return Literal()

class OR:
    def __invert__(self):
        return AND()

class AND:
    def __invert__(self):
        return OR()

def to_NNF(cond):
    if cond:
        return ~to_NNF(cond)
    if cond:
        return OR()
    if cond:
        return AND()
    return Literal()
