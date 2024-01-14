def func():
    import re

    # OK
    if re.match("^hello", "hello world", re.IGNORECASE):
        pass


def func():
    import re

    # FURB167
    if re.match("^hello", "hello world", re.I):
        pass


def func():
    from re import match, I

    # FURB167
    if match("^hello", "hello world", I):
        pass
