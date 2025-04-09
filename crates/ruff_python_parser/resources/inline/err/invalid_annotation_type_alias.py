type X[T: (yield 1)] = int      # TypeVar bound
type X[T = (yield 1)] = int     # TypeVar default
type X[*Ts = (yield 1)] = int   # TypeVarTuple default
type X[**Ts = (yield 1)] = int  # ParamSpec default
type Y = (yield 1)              # yield in value
type Y = (x := 1)               # named expr in value
type Y[T: (await 1)] = int      # await in bound
type Y = (await 1)              # await in value
