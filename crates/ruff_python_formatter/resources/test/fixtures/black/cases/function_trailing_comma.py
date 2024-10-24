def f(a,):
    d = {'key': 'value',}
    tup = (1,)

def f2(a,b,):
    d = {'key': 'value', 'key2': 'value2',}
    tup = (1,2,)

def f(a:int=1,):
    call(arg={'explode': 'this',})
    call2(arg=[1,2,3],)
    x = {
        "a": 1,
        "b": 2,
    }["a"]
    if a == {"a": 1,"b": 2,"c": 3,"d": 4,"e": 5,"f": 6,"g": 7,"h": 8,}["a"]:
        pass

def xxxxxxxxxxxxxxxxxxxxxxxxxxxx() -> Set[
    "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
]:
    json = {"k": {"k2": {"k3": [1,]}}}



# The type annotation shouldn't get a trailing comma since that would change its type.
# Relevant bug report: https://github.com/psf/black/issues/2381.
def some_function_with_a_really_long_name() -> (
    returning_a_deeply_nested_import_of_a_type_i_suppose
):
    pass


def some_method_with_a_really_long_name(very_long_parameter_so_yeah: str, another_long_parameter: int) -> (
    another_case_of_returning_a_deeply_nested_import_of_a_type_i_suppose_cause_why_not
):
    pass


def func() -> (
    also_super_long_type_annotation_that_may_cause_an_AST_related_crash_in_black(this_shouldn_t_get_a_trailing_comma_too)
):
    pass


def func() -> ((also_super_long_type_annotation_that_may_cause_an_AST_related_crash_in_black(
        this_shouldn_t_get_a_trailing_comma_too
    ))
):
    pass


# Make sure inner one-element tuple won't explode
some_module.some_function(
    argument1, (one_element_tuple,), argument4, argument5, argument6
)

# Inner trailing comma causes outer to explode
some_module.some_function(
    argument1, (one, two,), argument4, argument5, argument6
)

def foo() -> (
    # comment inside parenthesised return type
    int
):
    ...

def foo() -> (
    # comment inside parenthesised return type
    # more
    int
    # another
):
    ...

def foo() -> (
    # comment inside parenthesised new union return type
    int | str | bytes
):
    ...

def foo() -> (
    # comment inside plain tuple
):
    pass

def foo(arg: (# comment with non-return annotation
        int
        # comment with non-return annotation
)):
    pass

def foo(arg: (# comment with non-return annotation
        int | range | memoryview
        # comment with non-return annotation
)):
    pass

def foo(arg: (# only before
        int
)):
    pass

def foo(arg: (
        int
        # only after
)):
    pass

variable: ( # annotation
    because
    # why not
)

variable: (
    because
    # why not
)
