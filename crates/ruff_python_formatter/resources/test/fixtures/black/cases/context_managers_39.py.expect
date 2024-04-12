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
    new_new_new2(),
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


with (
    mock.patch.object(self.my_runner, "first_method", autospec=True) as mock_run_adb,
    mock.patch.object(
        self.my_runner, "second_method", autospec=True, return_value="foo"
    ),
):
    pass


with xxxxxxxx.some_kind_of_method(
    some_argument=[
        "first",
        "second",
        "third",
    ]
).another_method() as cmd:
    pass


async def func():
    async with (
        make_context_manager1() as cm1,
        make_context_manager2() as cm2,
        make_context_manager3() as cm3,
        make_context_manager4() as cm4,
    ):
        pass

    async with (
        some_function(argument1, argument2, argument3="some_value") as some_cm,
        some_other_function(argument1, argument2, argument3="some_value"),
    ):
        pass
