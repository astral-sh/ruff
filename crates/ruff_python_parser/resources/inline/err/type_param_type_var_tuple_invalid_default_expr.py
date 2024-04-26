type X[*Ts = *int] = int
type X[*Ts = *int or str] = int
type X[*Ts = yield x] = int
type X[*Ts = yield from x] = int
type X[*Ts = x := int] = int
