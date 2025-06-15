bla = b"bla"
d = {"a": b"bla", "b": b"bla", "c": b"bla"}


def foo(one_arg):
    pass


f"{str(bla)}, {repr(bla)}, {ascii(bla)}"  # RUF010

f"{str(d['a'])}, {repr(d['b'])}, {ascii(d['c'])}"  # RUF010

f"{(str(bla))}, {(repr(bla))}, {(ascii(bla))}"  # RUF010

f"{bla!s}, {(repr(bla))}, {(ascii(bla))}"  # RUF010

f"{foo(bla)}"  # OK

f"{str(bla, 'ascii')}, {str(bla, encoding='cp1255')}"  # OK

f"{bla!s} {[]!r} {'bar'!a}"  # OK

"Not an f-string {str(bla)}, {repr(bla)}, {ascii(bla)}"  # OK


def ascii(arg):
    pass


f"{ascii(bla)}"  # OK

(
    f"Member of tuple mismatches type at index {i}. Expected {of_shape_i}. Got "
    " intermediary content "
    f" that flows {repr(obj)} of type {type(obj)}.{additional_message}"  # RUF010
)


# https://github.com/astral-sh/ruff/issues/16325
f"{str({})}"
f"{ascii({} | {})}"

import builtins

f"{builtins.repr(1)}"
f"{ascii(1)=}"
f"{ascii(lambda: 1)}"
f"{ascii(x := 2)}"
