type X = int
type X = int | str
type X = int | "ForwardRefY"
type X[T] = T | list[X[T]]  # recursive
type X[T] = int
type X[T] = list[T] | set[T]
type X[T, *Ts, **P] = (T, Ts, P)
type X[T: int, *Ts, **P] = (T, Ts, P)
type X[T: (int, str), *Ts, **P] = (T, Ts, P)
type X[T = int] = T | str
type X[T: int | str = int] = T | int | str
type X[*Ts = *tuple[int, str]] = tuple[int, *Ts, str]
type X[**P = [int, str]] = Callable[P, str]

# Soft keyword as alias name
type type = int
type match = int
type case = int

# Soft keyword as value
type foo = type
type foo = match
type foo = case

# Multine definitions
type \
	X = int
type X \
	= int
type X = \
	int
type X = (
    int
)
type \
    X[T] = T
type X \
    [T] = T
type X[T] \
    = T

# Simple statements
type X = int; type X = str; type X = type
class X: type X = int

type Point = tuple[float, float]
type Point[T] = tuple[T, T]
type IntFunc[**P] = Callable[P, int]  # ParamSpec
type LabeledTuple[*Ts] = tuple[str, *Ts]  # TypeVarTuple
type HashableSequence[T: Hashable] = Sequence[T]  # TypeVar with bound
type IntOrStrSequence[T: (int, str)] = Sequence[T]  # TypeVar with constraints

# Type as an identifier
type *a + b, c   # ((type * a) + b), c
type *(a + b), c   # (type * (a + b)), c
type (*a + b, c)   # type ((*(a + b)), c)
type -a * b + c   # (type - (a * b)) + c
type -(a * b) + c   # (type - (a * b)) + c
type (-a) * b + c   # (type (-(a * b))) + c
type ().a   # (type()).a
type (()).a   # (type(())).a
type ((),).a   # (type(())).a
type [a].b   # (type[a]).b
type [a,].b   # (type[(a,)]).b  (not (type[a]).b)
type [(a,)].b   # (type[(a,)]).b
type()[a:
    b]  # (type())[a: b]
if type := 1: pass
type = lambda query: query == event
print(type(12))
type(type)
a = (
	type in C
)
a = (
	type(b)
)
type (
	X = int
)
type = 1
type = x = 1
x = type = 1
lambda x: type