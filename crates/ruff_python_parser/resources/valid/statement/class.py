class Test:
    ...


class Test():
        def __init__(self):
            pass


class Test(a=1, *A, **k):
    ...


class Test:
    def method():
        a, b = data


class Test(A, B):
    def __init__(self):
        pass

    def method_with_default(self, arg='default'):
        pass


# Class with generic types:

# TypeVar
class Test[T](): ...

# TypeVar with bound
class Test[T: str](): ...

# TypeVar with tuple bound
class Test[T: (str, bytes)](): ...

# Multiple TypeVar
class Test[T, U](): ...

# Trailing comma
class Test[T, U,](): ...

# TypeVarTuple
class Test[*Ts](): ...

# ParamSpec
class Test[**P](): ...

# Mixed types
class Test[X, Y: str, *U, **P]():
  pass
