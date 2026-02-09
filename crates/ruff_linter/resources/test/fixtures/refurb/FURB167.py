def func():
    import re

    # OK
    if re.search("^hello", "hello world", re.IGNORECASE):
        pass


def func():
    import re

    # FURB167
    if re.search("^hello", "hello world", re.I):
        pass


def func():
    from re import search, I

    # FURB167
    if search("^hello", "hello world", I):
        pass
