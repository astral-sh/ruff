def no_parameters():
    pass


def positional_parameters(a, b, c):
    pass


def positional_parameters_with_default_values(a, b=20, c=30):
    pass


def positional_parameters_with_default_values2(a, b=20, /, c=30):
    pass


def positional_only_and_positional_parameters(a, /, b, c):
    pass


def pos_args_with_defaults_and_varargs_and_kwargs(a, b=20, /, c=30, *args, **kwargs):
    pass


def keyword_only_parameters(*, a, b, c):
    pass


def keyword_only_parameters_with_defaults(*, a, b=20, c=30):
    pass


def kw_only_args_with_defaults_and_varargs(*args, a, b=20, c=30):
    pass


def kw_only_args_with_defaults_and_kwargs(*, a, b=20, c=30, **kwargs):
    pass


def kw_only_args_with_defaults_and_varargs_and_kwargs(*args, a, b=20, c=30, **kwargs):
    pass


def pos_and_kw_only_args(a, b, /, c, *, d, e, f):
    pass


def pos_and_kw_only_args_with_defaults(a, b, /, c, *, d, e=20, f=30):
    pass


def pos_and_kw_only_args_with_defaults_and_varargs(a, b, /, c, *args, d, e=20, f=30):
    pass


def pos_and_kw_only_args_with_defaults_and_kwargs(
    a, b, /, c, *, d, e=20, f=30, **kwargs
):
    pass


def pos_and_kw_only_args_with_defaults_and_varargs_and_kwargs(
    a, b, /, c, *args, d, e=20, f=30, **kwargs
):
    pass


def positional_and_keyword_parameters(a, b, c, *, d, e, f):
    pass


def positional_and_keyword_parameters_with_defaults(a, b, c, *, d, e=20, f=30):
    pass


def positional_and_keyword_parameters_with_defaults_and_varargs(
    a, b, c, *args, d, e=20, f=30
):
    pass


def positional_and_keyword_parameters_with_defaults_and_varargs_and_kwargs(
    a, b, c, *args, d, e=20, f=30, **kwargs
):
    pass


# Function definitions with type parameters


def func[T](a: T) -> T:
    pass


def func[T: str](a: T) -> T:
    pass


def func[T: (str, bytes)](a: T) -> T:
    pass


def func[*Ts](*a: *Ts) -> Tuple[*Ts]:
    pass


def func[**P](*args: P.args, **kwargs: P.kwargs):
    pass


def func[T, U: str, *Ts, **P]():
    pass


def ellipsis(): ...


def multiple_statements() -> int:
    call()
    pass
    ...


def foo(*args):
    pass


def foo(**kwargs):
    pass


def foo(*args, **kwargs):
    pass


def foo(a, /):
    pass


def foo(a, /, b):
    pass


def foo(a=1, /,):
    pass


def foo(a, b, /, *, c):
    pass


def foo(kw=1, *, a):
    pass


def foo(x: int, y: "str", z: 1 + 2):
    pass


def foo(self, a=1, b=2, c=3):
    pass
