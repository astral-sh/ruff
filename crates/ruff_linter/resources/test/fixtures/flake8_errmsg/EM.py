from __future__ import annotations


def f_a():
    raise RuntimeError("This is an example exception")


def f_a_short():
    raise RuntimeError("Error")


def f_a_empty():
    raise RuntimeError("")


def f_b():
    example = "example"
    raise RuntimeError(f"This is an {example} exception")


def f_c():
    raise RuntimeError("This is an {example} exception".format(example="example"))


def f_ok():
    msg = "hello"
    raise RuntimeError(msg)


def f_msg_defined():
    msg = "hello"
    raise RuntimeError("This is an example exception")


def f_msg_in_nested_scope():
    def nested():
        msg = "hello"

    raise RuntimeError("This is an example exception")


def f_msg_in_parent_scope():
    msg = "hello"

    def nested():
        raise RuntimeError("This is an example exception")


def f_fix_indentation_check(foo):
    if foo:
        raise RuntimeError("This is an example exception")
    else:
        if foo == "foo":
            raise RuntimeError(f"This is an exception: {foo}")
    raise RuntimeError("This is an exception: {}".format(foo))


# Report these, but don't fix them
if foo: raise RuntimeError("This is an example exception")
if foo: x = 1; raise RuntimeError("This is an example exception")


def f_triple_quoted_string():
    raise RuntimeError(f"""This is an {"example"} exception""")


def f_multi_line_string():
    raise RuntimeError(
        "first"
        "second"
    )


def f_multi_line_string2():
    raise RuntimeError(
        "This is an {example} exception".format(
            example="example"
        )
    )


def f_multi_line_string2():
    raise RuntimeError(
        (
            "This is an "
            "{example} exception"
        ).format(
            example="example"
        )
    )


def raise_typing_cast_exception():
    import typing
    raise typing.cast("Exception", None)


def f_typing_cast_excluded():
    from typing import cast
    raise cast(RuntimeError, "This should not trigger EM101")


def f_typing_cast_excluded_import():
    import typing
    raise typing.cast(RuntimeError, "This should not trigger EM101")


def f_typing_cast_excluded_aliased():
    from typing import cast as my_cast
    raise my_cast(RuntimeError, "This should not trigger EM101")


