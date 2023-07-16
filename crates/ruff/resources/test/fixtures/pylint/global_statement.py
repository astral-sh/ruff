# Adapted from:
#   https://github.com/PyCQA/pylint/blob/b70d2abd7fabe9bfd735a30b593b9cd5eaa36194/tests/functional/g/globals.py

CONSTANT = 1


def FUNC():
    pass


class CLASS:
    pass


def fix_constant(value):
    """All this is ok, but try not to use `global` ;)"""
    global CONSTANT  # [global-statement]
    print(CONSTANT)
    CONSTANT = value


def global_with_import():
    """Should only warn for global-statement when using `Import` node"""
    global sys  # [global-statement]
    import sys


def global_with_import_from():
    """Should only warn for global-statement when using `ImportFrom` node"""
    global namedtuple  # [global-statement]
    from collections import namedtuple


def global_del():
    """Deleting the global name prevents `global-variable-not-assigned`"""
    global CONSTANT  # [global-statement]
    print(CONSTANT)
    del CONSTANT


def global_operator_assign():
    """Operator assigns should only throw a global statement error"""
    global CONSTANT  # [global-statement]
    print(CONSTANT)
    CONSTANT += 1


def global_function_assign():
    """Function assigns should only throw a global statement error"""
    global CONSTANT  # [global-statement]

    def CONSTANT():
        pass

    CONSTANT()


def override_func():
    """Overriding a function should only throw a global statement error"""
    global FUNC  # [global-statement]

    def FUNC():
        pass

    FUNC()


def override_class():
    """Overriding a class should only throw a global statement error"""
    global CLASS  # [global-statement]

    class CLASS:
        pass

    CLASS()


def multiple_assignment():
    """Should warn on every assignment."""
    global CONSTANT  # [global-statement]
    CONSTANT = 1
    CONSTANT = 2


def no_assignment():
    """Shouldn't warn"""
    global CONSTANT
