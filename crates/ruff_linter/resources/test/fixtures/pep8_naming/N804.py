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

    def __init__(self): ...

    def __new__(cls, *args, **kwargs): ...

    def __init_subclass__(self, default_name, **kwargs): ...

    @classmethod
    def class_method_with_positional_only_argument(cls, x, /, other): ...

    @classmethod
    def bad_class_method_with_positional_only_argument(self, x, /, other): ...


class MetaClass(ABCMeta):
    def bad_method(self):
        pass

    def good_method(cls):
        pass

    @staticmethod
    def static_method(not_cls) -> bool:
        return False


class ClsInArgsClass(ABCMeta):
    def cls_as_argument(this, cls):
        pass

    def cls_as_pos_only_argument(this, cls, /):
        pass

    def cls_as_kw_only_argument(this, *, cls):
        pass

    def cls_as_varags(this, *cls):
        pass

    def cls_as_kwargs(this, **cls):
        pass


class RenamingInMethodBodyClass(ABCMeta):
    def bad_method(this):
        this = this
        this

    def bad_method(this):
        self = this


def func(x):
    return x


class Empty:
    def empty_method():
        pass

    @classmethod
    def empty_class_method():
        pass

    @staticmethod
    def empty_static_method():
        pass

    def no_positional_method(*, self):
        pass

    @classmethod
    def no_positional_class_method(*, cls):
        pass

    @staticmethod
    def no_positional_static_method(*, cls):
        pass

    def vararg_method(*args):
        pass

    def kwarg_method(**kwargs):
        pass
