# The input source must not contain any Py36-specific syntax (e.g. argument type
# annotations, trailing comma after *rest) or this test becomes invalid.
def long_function_name(argument_one, argument_two, argument_three, argument_four, argument_five, argument_six, *rest): ...
# output
# The input source must not contain any Py36-specific syntax (e.g. argument type
# annotations, trailing comma after *rest) or this test becomes invalid.
def long_function_name(
    argument_one,
    argument_two,
    argument_three,
    argument_four,
    argument_five,
    argument_six,
    *rest,
):
    ...
