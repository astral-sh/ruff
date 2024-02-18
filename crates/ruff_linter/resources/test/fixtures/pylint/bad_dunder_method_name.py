from typing import override


class Apples:
    def _init_(self):  # [bad-dunder-name]
        pass

    def __hello__(self):  # [bad-dunder-name]
        print("hello")

    def __init_(self):  # [bad-dunder-name]
        # author likely unintentionally misspelled the correct init dunder.
        pass

    def _init_(self):  # [bad-dunder-name]
        # author likely unintentionally misspelled the correct init dunder.
        pass

    def ___neg__(self):  # [bad-dunder-name]
        # author likely accidentally added an additional `_`
        pass

    def __inv__(self):  # [bad-dunder-name]
        # author likely meant to call the invert dunder method
        pass

    @override
    def _ignore__(self):  # [bad-dunder-name]
        # overridden dunder methods should be ignored
        pass

    def hello(self):
        print("hello")

    def __init__(self):
        pass

    def init(self):
        # valid name even though someone could accidentally mean __init__
        pass

    def _protected_method(self):
        print("Protected")

    def __private_method(self):
        print("Private")

    @property
    def __doc__(self):
        return "Docstring"

    # Added in Python 3.12
    def __buffer__(self):
        return memoryview(b'')

    def __release_buffer__(self, buf):
        pass

    # Allow dunder methods recommended by attrs.
    def __attrs_post_init__(self):
        pass

    def __attrs_pre_init__(self):
        pass

    def __attrs_init__(self):
        pass

    # Allow __html__, used by Jinja2 and Django.
    def __html__(self):
        pass

    # Allow Python's __index__
    def __index__(self):
        pass

    # Allow _missing_, used by enum.Enum.
    @classmethod
    def _missing_(cls, value):
        pass

    # Allow anonymous functions.
    def _(self):
        pass

    # Allow custom dunder names (via setting).
    def __special_custom_magic__(self):
        pass

    @classmethod
    def __prepare__():
        pass

    def __mro_entries__(self, bases):
        pass


def __foo_bar__():  # this is not checked by the [bad-dunder-name] rule
    ...
