type Alias[T, T] = ...
def f[T, T](t: T): ...
class C[T, T]: ...
type Alias[T, U: str, V: (str, bytes), *Ts, **P, T = default] = ...
def f[T, T, T](): ...  # two errors
def f[T, *T](): ...    # star is still duplicate
def f[T, **T](): ...   # as is double star
