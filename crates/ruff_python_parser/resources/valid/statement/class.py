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

# TypeVar with default
class Test[T = str](): ...

# TypeVar with bound
class Test[T: str](): ...

# TypeVar with bound and default
class Test[T: int | str = int](): ...

# TypeVar with tuple bound
class Test[T: (str, bytes)](): ...

# Multiple TypeVar
class Test[T, U](): ...

# Trailing comma
class Test[T, U,](): ...

# TypeVarTuple
class Test[*Ts](): ...

# TypeVarTuple with default
class Test[*Ts = Unpack[tuple[int, str]]](): ...

# TypeVarTuple with starred default
class Test[*Ts = *tuple[int, str]](): ...

# ParamSpec
class Test[**P](): ...

# ParamSpec with default
class Test[**P = [int, str]](): ...

# Mixed types
class Test[X, Y: str, *U, **P]():
  pass
