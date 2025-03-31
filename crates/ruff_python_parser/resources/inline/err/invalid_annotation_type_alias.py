type X[T: (yield 1)] = int      # TypeVar bound
type X[T = (yield 1)] = int     # TypeVar default
type X[*Ts = (yield 1)] = int   # TypeVarTuple default
type X[**Ts = (yield 1)] = int  # ParamSpec default
type Y = (yield 1)              # value
