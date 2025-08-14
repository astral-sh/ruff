def simple_function(arg1, arg2, arg3):
    pass

def single_argument(
    arg1,
):
    pass

def multiple_arguments(
    arg1,
    arg2,
    arg3,
    arg4,
):
    pass

def function_with_defaults(
    arg1="default",
    arg2=None,
    arg3=[1, 2, 3],
):
    pass

def function_with_long_names(
    very_long_argument_name_that_might_cause_wrapping,
    another_very_long_argument_name,
    yet_another_extremely_long_argument_name,
):
    pass

def mixed_argument_types(
    arg1,
    arg2="default",
    *args,
    kwarg1=None,
    kwarg2="another_default",
    **kwargs,
):
    pass

def positional_only_args(
    arg1,
    arg2,
    /,
    arg3,
    arg4,
):
    pass

def keyword_only_args(
    arg1,
    arg2,
    *,
    kwarg1,
    kwarg2,
):
    pass

def all_argument_types(
    pos1,
    pos2,
    /,
    normal1,
    normal2,
    *args,
    kw1,
    kw2="default",
    **kwargs,
):
    pass

# Test lambdas (these should not be affected)
lambda_func = lambda arg1, arg2, arg3: arg1 + arg2 + arg3

# Nested function
def outer_function(
    outer_arg1,
    outer_arg2,
):
    def inner_function(
        inner_arg1,
        inner_arg2,
        inner_arg3,
    ):
        return inner_arg1 + inner_arg2 + inner_arg3
    
    return inner_function(outer_arg1, outer_arg2, 0)