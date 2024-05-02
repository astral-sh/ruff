import abc

import pydantic


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

    @abc.abstractclassmethod
    def abstract_class_method(cls):
        pass

    @staticmethod
    def static_method(x):
        return x

    @pydantic.validator
    def lower(cls, my_field: str) -> str:
        pass

    @pydantic.validator("my_field")
    def lower(cls, my_field: str) -> str:
        pass

    def __init__(self):
        ...

    def __new__(cls, *args, **kwargs):
        ...

    def __init_subclass__(self, default_name, **kwargs):
        ...


class MetaClass(abc.ABCMeta):
    def bad_method(self):
        pass

    def good_method(cls):
        pass


def func(x):
    return x


class PosOnlyClass:
    def good_method_pos_only(self, blah, /, something: str):
        pass

    def bad_method_pos_only(this, blah, /, something: str):
        pass


class ModelClass:
    @hybrid_property
    def bad(cls):
        pass

    @bad.expression
    def bad(self):
        pass

    @bad.wtf
    def bad(cls):
        pass

    @hybrid_property
    def good(self):
        pass

    @good.expression
    def good(cls):
        pass

    @good.wtf
    def good(self):
        pass

    @foobar.thisisstatic
    def badstatic(foo):
        pass


class SelfInArgsClass:
    def self_as_argument(this, self):
        pass

    def self_as_pos_only_argument(this, self, /):
        pass

    def self_as_kw_only_argument(this, *, self):
        pass

    def self_as_varags(this, *self):
        pass

    def self_as_kwargs(this, **self):
        pass


class RenamingInMethodBodyClass:
    def bad_method(this):
        this = this
        this

    def bad_method(this):
        self = this


class RenamingWithNFKC:
    def formula(household):
        hºusehold(1)
