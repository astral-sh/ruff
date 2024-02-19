def no_parameters():
    pass


def positional_parameters(a, b, c):
    pass


def positional_parameters_with_default_values(a, b=20, c=30):
    pass


def keyword_only_parameters(*, a, b, c):
    pass


def keyword_only_parameters_with_defaults(*, a, b=20, c=30):
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
