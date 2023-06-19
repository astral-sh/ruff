def f(target):
    """Can be refactored to `return target in {"foo", "bar", "baz"}`."""
    return target == "foo" or target == "bar" or target == "baz"  # Error.


def f(target):
    """Same for yoda conditions."""
    return "foo" == target or "bar" == target or "baz" == target  # Error.


def f(target):
    """Same for a mix of yoda conditions and 'normal' conditions."""
    return "foo" == target or target == "bar" or "baz" == target  # Error.


def f(target):
    return target in {"foo", "bar", "baz"}  # Fixed.


def f(target):
    """Can be refactored to `return target not in {"foo", "bar", "baz"}`."""
    return target != "foo" and target != "bar" and target != "baz"  # Error.


def f(target):
    return target not in {"foo", "bar", "baz"}  # Fixed.


def f(target):
    """Cannot be refactored because of the `and` comparisons."""
    return target == "foo" and target == "bar" and target == "baz"


def f(target):
    """Cannot be refactored because of the `and` comparisons."""
    return target == "foo" and target == "bar" or target == "baz"


def f(target):
    """Combines `!=` and `==`, so cannot be fixed."""
    return target != "foo" or target != "bar" or target == "baz"

def f(target):
    """One check should be ignored."""
    return target == "foo"

def f(target):
    """One check should be ignored."""
    return target != "foo"

def f(target):
    """Long chain of comparisons with an `and` at the start."""
    return target == 1 and target == 2 or target == 3 or target == 4 or target == 5 or target == 6 or target == 7 or target == 8 or target == 9 or target == 10 or target == 11

def f(target):
    """Long chain of comparisons with an `and` at the end."""
    return target == 1 or target == 2 or target == 3 or target == 4 or target == 5 or target == 6 or target == 7 or target == 8 or target == 9 or target == 10 and target == 11

def f(target):
    """"Long chain of comparisons with an `and` in the middle."""
    return target == 1 or target == 2 or target == 3 or target == 4 or target == 5 and target == 6 or target == 7 or target == 8 or target == 9 or target == 10 or target == 11

def f(target):
    """Long chain of `or`s."""
    return target == 1 or target == 2 or target == 3 or target == 4 or target == 5 or target == 6 or target == 7 or target == 8 or target == 9 or target == 10 or target == 11