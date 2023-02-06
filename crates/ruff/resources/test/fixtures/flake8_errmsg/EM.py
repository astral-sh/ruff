from __future__ import annotations


def f_a():
    raise RuntimeError("This is an example exception")


def f_a_short():
    raise RuntimeError("Error")


def f_b():
    example = "example"
    raise RuntimeError(f"This is an {example} exception")


def f_c():
    raise RuntimeError("This is an {example} exception".format(example="example"))


def f_ok():
    msg = "hello"
    raise RuntimeError(msg)
