# pylint: disable=invalid-name, import-outside-toplevel, too-few-public-methods, unused-import
# pylint: disable=missing-module-docstring, missing-function-docstring, missing-class-docstring
# pylint: disable=global-at-module-level, global-statement, global-variable-not-assigned
CONSTANT = 1
UNDEFINED: int


def FUNC():
    pass


class CLASS:
    pass


# BAD
def global_variable_undefined():
    global SOMEVAR  # [global-variable-undefined]
    SOMEVAR = 2


# OK
def global_constant():
    global CONSTANT
    print(CONSTANT)
    global UNDEFINED
    UNDEFINED = 1
    global CONSTANT_2
    print(CONSTANT_2)


def global_with_import():
    global sys
    import sys


def global_with_import_from():
    global namedtuple
    from collections import namedtuple


def override_func():
    global FUNC

    def FUNC():
        pass

    FUNC()


def override_class():
    global CLASS

    class CLASS():
        pass

    CLASS()


CONSTANT_2 = 2
