type X[T: *int] = int
type X[T: yield x] = int
type X[T: yield from x] = int
type X[T: x := int] = int
