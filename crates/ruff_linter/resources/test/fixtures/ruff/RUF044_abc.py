from abc import ABC, abstractmethod, abstractproperty, abstractstaticmethod, abstractclassmethod


class A(ABC):
    @abstractmethod
    def abstract_instance_method(self): ...

    @abstractclassmethod
    def abstract_class_method(cls): ...

    @abstractstaticmethod
    def abstract_static_method(): ...

    @abstractproperty
    def abstract_property(self): ...


class B: ...


class C(B, A):
    @abstractmethod
    def abstract_instance_method(self): ...

    @abstractclassmethod
    def abstract_class_method(cls): ...

    @abstractstaticmethod
    def abstract_static_method(): ...

    @abstractproperty
    def abstract_property(self): ...
