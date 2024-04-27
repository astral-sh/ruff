type X[T] = int
type X[T = int] = int
type X[T: int = int] = int
type X[T: (int, int) = int] = int
type X[T: int = int, U: (int, int) = int] = int
