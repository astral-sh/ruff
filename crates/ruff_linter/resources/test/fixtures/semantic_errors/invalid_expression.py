type X[T: (yield 1)] = int

type Y = (yield 1)

def f[T](x: int) -> (y := 3): return x

class C[T]((yield from [object])):
    pass