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


def f_unfixable():
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
