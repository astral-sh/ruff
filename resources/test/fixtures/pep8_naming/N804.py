from abc import ABCMeta


class Class:
    def bad_method(this):
        pass

    if False:

        def extra_bad_method(this):
            pass

    def good_method(self):
        pass

    @classmethod
    def class_method(cls):
        pass

    @staticmethod
    def static_method(x):
        return x

    def __init__(self):
        ...

    def __new__(cls, *args, **kwargs):
        ...

    def __init_subclass__(self, default_name, **kwargs):
        ...

    @classmethod
    def class_method_with_positional_only_argument(cls, x, /, other):
        ...

    @classmethod
    def bad_class_method_with_positional_only_argument(self, x, /, other):
        ...


class MetaClass(ABCMeta):
    def bad_method(self):
        pass

    def good_method(cls):
        pass


def func(x):
    return x
