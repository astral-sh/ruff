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

f"{str({} | {})}"

import builtins

f"{builtins.repr(1)}"

f"{repr(1)=}"

f"{repr(lambda: 1)}"

f"{repr(x := 2)}"

f"{str(object=3)}"

f"{str(x for x in [])}"

f"{str((x for x in []))}"

# test f-strings with comments

## SAFE CASES
f"{ascii((  # comment inside
    1
))}"

f"{ascii([
    1,  # first item
    2  # second item
])}"

f"{repr({
    'a': 1,  # comment 1
    'b': 2,  # comment 2
})}"

f"{ascii((
    [1, 2, 3][  # accessing list
        0  # first element
    ]
))}"

f"{str(
    some_function(
        arg1,  # first argument
        arg2  # second argument
    ) + other_value  # addition
)}"

## UNSAFE CASES
f"{ascii  # this comment will be lost
(1)}"

f"{str  # comment here
(my_var)}"

f"{repr
# this comment is lost
(value)}"
