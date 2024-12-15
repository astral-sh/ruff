from abc import ABCMeta, abstractmethod, abstractproperty, abstractstaticmethod, abstractclassmethod


class M(ABCMeta): ...


class C(metaclass=M):
    @abstractmethod
    def abstract_instance_method(self): ...

    @abstractclassmethod
    def abstract_class_method(cls): ...

    @abstractstaticmethod
    def abstract_static_method(): ...

    @abstractproperty
    def abstract_property(self): ...


class D(C):
    @abstractmethod
    def abstract_instance_method(self): ...

    @abstractclassmethod
    def abstract_class_method(cls): ...

    @abstractstaticmethod
    def abstract_static_method(): ...

    @abstractproperty
    def abstract_property(self): ...
