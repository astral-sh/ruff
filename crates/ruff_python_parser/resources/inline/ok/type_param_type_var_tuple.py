type X[*Ts] = int
type X[*Ts = int] = int
type X[*Ts = *int] = int
type X[T, *Ts] = int
type X[T, *Ts = int] = int
