with \
     make_context_manager1() as cm1, \
     make_context_manager2() as cm2, \
     make_context_manager3() as cm3, \
     make_context_manager4() as cm4 \
:
    pass


# Leading comment
with \
     make_context_manager1() as cm1, \
     make_context_manager2(), \
     make_context_manager3() as cm3, \
     make_context_manager4() \
:
    pass


with \
     new_new_new1() as cm1, \
     new_new_new2() \
:
    pass


with (
     new_new_new1() as cm1,
     new_new_new2()
):
    pass


# Leading comment.
with (
     # First comment.
     new_new_new1() as cm1,
     # Second comment.
     new_new_new2()
     # Last comment.
):
    pass


with \
    this_is_a_very_long_call(looong_arg1=looong_value1, looong_arg2=looong_value2) as cm1, \
    this_is_a_very_long_call(looong_arg1=looong_value1, looong_arg2=looong_value2, looong_arg3=looong_value3, looong_arg4=looong_value4) as cm2 \
:
    pass


# output


with (
    make_context_manager1() as cm1,
    make_context_manager2() as cm2,
    make_context_manager3() as cm3,
    make_context_manager4() as cm4,
):
    pass


# Leading comment
with (
    make_context_manager1() as cm1,
    make_context_manager2(),
    make_context_manager3() as cm3,
    make_context_manager4(),
):
    pass


with new_new_new1() as cm1, new_new_new2():
    pass


with new_new_new1() as cm1, new_new_new2():
    pass


# Leading comment.
with (
    # First comment.
    new_new_new1() as cm1,
    # Second comment.
    new_new_new2()
    # Last comment.
):
    pass


with (
    this_is_a_very_long_call(
        looong_arg1=looong_value1, looong_arg2=looong_value2
    ) as cm1,
    this_is_a_very_long_call(
        looong_arg1=looong_value1,
        looong_arg2=looong_value2,
        looong_arg3=looong_value3,
        looong_arg4=looong_value4,
    ) as cm2,
):
    pass
