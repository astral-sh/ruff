from abc import ABC, abstractmethod, abstractproperty, abstractstaticmethod, abstractclassmethod


class NoBase:
    @abstractmethod
    def abstract_instance_method(self): ...

    @abstractclassmethod
    def abstract_class_method(cls): ...

    @abstractstaticmethod
    def abstract_static_method(): ...

    @abstractproperty
    def abstract_property(self): ...


class HasBase(NoBase):
    @abstractmethod
    def abstract_instance_method(self): ...

    @abstractclassmethod
    def abstract_class_method(cls): ...

    @abstractstaticmethod
    def abstract_static_method(): ...

    @abstractproperty
    def abstract_property(self): ...


class HasMetaclass(metaclass=HasBase):
    @abstractmethod
    def abstract_instance_method(self): ...

    @abstractclassmethod
    def abstract_class_method(cls): ...

    @abstractstaticmethod
    def abstract_static_method(): ...

    @abstractproperty
    def abstract_property(self): ...
