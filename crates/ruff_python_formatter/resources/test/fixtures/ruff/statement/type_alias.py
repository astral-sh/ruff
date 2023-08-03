# basic usage

type X = int
type X = int | str
type X = int | "ForwardRefY"
type X[T] = T | list[X[T]]  # recursive
type X[T] = int
type X[T] = list[T] | set[T]
type X[T, *Ts, **P] = (T, Ts, P)
type X[T: int, *Ts, **P] = (T, Ts, P)
type X[T: (int, str), *Ts, **P] = (T, Ts, P)

# long name
type Xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx = int
type Xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx[A] = int
type Xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx[Aaaaaaaaaaaaaaaaaaaaaaaaaaaa] = int
type Xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx[Aaaaaaaaaaaaaaaaaaaaaaaaaaaa, Bbbbbbbbbbbbb] = int
type Xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx = Tttttttttttttttttttttttttttttttttttttttttttttttttttttttt

# long value
type X = Ttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttt
type X = Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa | Bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb | Ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc

# soft keyword as alias name
type type = int
type match = int
type case = int

# soft keyword as value
type foo = type
type foo = match
type foo = case

# multine definitions
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

# type leading comment
type X = ( # trailing open paren comment
    # value leading comment
    int # value trailing comment
    # leading close paren comment
) # type trailing comment


# type leading comment
type X = (
    # value leading comment
    int # value trailing comment

    # leading close paren comment
)

# type parameters
type type_params_single_line[aaaaaaaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbb, ccccccccccccccccc] = int
type type_params_arguments_on_their_own_line[aaaaaaaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbb, ccccccccccc, ddddddddddddd, eeeeeee] = int
type type_params_argument_per_line[aaaaaaaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbb, ccccccccccccccccc, ddddddddddddd, eeeeeeeeeeeeeeee, ffffffffffff] = int
type type_params_trailing_comma[a, b,] = int
type type_params_comments[ # trailing open bracket comment
    # leading comment
    A,

    # in between comment

    B,
    # another leading comment
    C,
    D, # trailing comment
    # leading close bracket comment
] = int  # trailing value comment
type type_params_all_kinds[type_var, *type_var_tuple, **param_spec] = int

# type variable bounds
type bounds_single_line[T: (aaaaaaaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbb, ccccccccccccccccc)] = T
type bounds_arguments_on_their_own_line[T: (aaaaaaaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbb, ccccccccccc, ddddddddddddd, eeeeeee)] = T
type bounds_argument_per_line[T: (aaaaaaaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbb, ccccccccccccccccc, ddddddddddddd, eeeeeeeeeeeeeeee, ffffffffffff)] = T
type bounds_trailing_comma[T: (a, b,)] = T
