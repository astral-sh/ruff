type X[*Ts = *int] = int
type X[*Ts = yield x] = int
type X[*Ts = yield from x] = int
type X[*Ts = x := int] = int
