"""
Violation:

Prefer TypeError when relevant.
"""


def incorrect_basic(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_multiple_type_check(some_arg):
    if isinstance(some_arg, (int, str)):
        pass
    else:
        raise Exception("...")  # should be typeerror


class MyClass:
    pass


def incorrect_with_issubclass(some_arg):
    if issubclass(some_arg, MyClass):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_with_callable(some_arg):
    if callable(some_arg):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_ArithmeticError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise ArithmeticError("...")  # should be typeerror


def incorrect_AssertionError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise AssertionError("...")  # should be typeerror


def incorrect_AttributeError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise AttributeError("...")  # should be typeerror


def incorrect_BufferError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise BufferError  # should be typeerror


def incorrect_EOFError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise EOFError("...")  # should be typeerror


def incorrect_ImportError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise ImportError("...")  # should be typeerror


def incorrect_LookupError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise LookupError("...")  # should be typeerror


def incorrect_MemoryError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        # should be typeerror
        # not multiline is on purpose for fix
        raise MemoryError(
            "..."
        )


def incorrect_NameError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise NameError("...")  # should be typeerror


def incorrect_ReferenceError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise ReferenceError("...")  # should be typeerror


def incorrect_RuntimeError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise RuntimeError("...")  # should be typeerror


def incorrect_SyntaxError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise SyntaxError("...")  # should be typeerror


def incorrect_SystemError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise SystemError("...")  # should be typeerror


def incorrect_ValueError(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise ValueError("...")  # should be typeerror


def incorrect_not_operator_isinstance(some_arg):
    if not isinstance(some_arg):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_and_operator_isinstance(arg1, arg2):
    if isinstance(some_arg) and isinstance(arg2):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_or_operator_isinstance(arg1, arg2):
    if isinstance(some_arg) or isinstance(arg2):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_multiple_operators_isinstance(arg1, arg2, arg3):
    if not isinstance(arg1) and isinstance(arg2) or isinstance(arg3):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_not_operator_callable(some_arg):
    if not callable(some_arg):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_and_operator_callable(arg1, arg2):
    if callable(some_arg) and callable(arg2):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_or_operator_callable(arg1, arg2):
    if callable(some_arg) or callable(arg2):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_multiple_operators_callable(arg1, arg2, arg3):
    if not callable(arg1) and callable(arg2) or callable(arg3):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_not_operator_issubclass(some_arg):
    if not issubclass(some_arg):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_and_operator_issubclass(arg1, arg2):
    if issubclass(some_arg) and issubclass(arg2):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_or_operator_issubclass(arg1, arg2):
    if issubclass(some_arg) or issubclass(arg2):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_multiple_operators_issubclass(arg1, arg2, arg3):
    if not issubclass(arg1) and issubclass(arg2) or issubclass(arg3):
        pass
    else:
        raise Exception("...")  # should be typeerror


def incorrect_multi_conditional(arg1, arg2):
    if isinstance(arg1, int):
        pass
    elif isinstance(arg2, int):
        raise Exception("...")  # should be typeerror


class MyCustomTypeValidation(Exception):
    pass


def correct_custom_exception(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise MyCustomTypeValidation("...")  # that's correct, because it's not vanilla


def correct_complex_conditional(val):
    if val is not None and (not isinstance(val, int) or val < 0):
        raise ValueError(...)  # fine if this is not a TypeError


def correct_multi_conditional(some_arg):
    if some_arg == 3:
        pass
    elif isinstance(some_arg, int):
        pass
    else:
        raise Exception("...")  # fine if this is not a TypeError


def correct_should_ignore(some_arg):
    if isinstance(some_arg, int):
        pass
    else:
        raise TypeError("...")


def check_body(some_args):
    if isinstance(some_args, int):
        raise ValueError("...") # should be typeerror


def check_body_correct(some_args):
    if isinstance(some_args, int):
        raise TypeError("...") # correct


def multiple_elifs(some_args):
    if not isinstance(some_args, int):
        raise ValueError("...") # should be typerror
    elif some_args < 3:
        raise ValueError("...")  # this is ok
    elif some_args > 10:
        raise ValueError("...")  # this is ok if we don't simplify
    else:
        pass


def multiple_ifs(some_args):
    if not isinstance(some_args, int):
        raise ValueError("...") # should be typerror
    else:
        if some_args < 3:
            raise ValueError("...")  # this is ok
        else:
            if some_args > 10:
                raise ValueError("...")  # this is ok if we don't simplify
            else:
                pass


def early_return():
    if isinstance(this, some_type):
        if x in this:
            return

        raise ValueError(f"{this} has a problem")  # this is ok


def early_break():
    for x in this:
        if isinstance(this, some_type):
            if x in this:
                break

            raise ValueError(f"{this} has a problem")  # this is ok


def early_continue():
    for x in this:
        if isinstance(this, some_type):
            if x in this:
                continue

            raise ValueError(f"{this} has a problem")  # this is ok


def early_return_else():
    if isinstance(this, some_type):
        pass
    else:
        if x in this:
            return

        raise ValueError(f"{this} has a problem")  # this is ok
