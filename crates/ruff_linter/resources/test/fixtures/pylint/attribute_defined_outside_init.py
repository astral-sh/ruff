# Test cases for attribute-defined-outside-init (PLW0201)

# Should trigger: attribute defined outside __init__
class Student:
    def register(self):
        self.is_registered = True  # [attribute-defined-outside-init]


# Should not trigger: attribute defined in __init__
class StudentGood:
    def __init__(self):
        self.is_registered = False

    def register(self):
        self.is_registered = True  # This is fine, attribute was initialized in __init__


# Should not trigger: class variable
class StudentWithClassVar:
    is_registered = False

    def register(self):
        self.is_registered = True  # This is fine, it's a class variable


# Should not trigger: inherits from another class (might define attribute)
class SomeBaseClass:
    def __init__(self):
        self.is_registered = False


class StudentWithParent(SomeBaseClass):
    def register(self):
        self.is_registered = True  # Should not trigger, parent might define it


# Should not trigger: class with decorator
from dataclasses import dataclass


@dataclass
class StudentDataclass:
    is_registered: bool = False

    def register(self):
        self.is_registered = True  # Should not trigger, decorator might define it


# Should trigger: inherits from object explicitly (object inheritance is allowed)
class StudentWithObject(object):
    def register(self):
        self.is_registered = True  # [attribute-defined-outside-init]


# Should trigger: multiple attributes defined outside __init__
class MultipleAttributes:
    def setup(self):
        self.name = "test"  # [attribute-defined-outside-init]
        self.age = 25  # [attribute-defined-outside-init]


# Should trigger: annotated assignment outside __init__
class AnnotatedAssignment:
    def setup(self):
        self.name: str = "test"  # [attribute-defined-outside-init]


# Should trigger: augmented assignment outside __init__
class AugmentedAssignment:
    def setup(self):
        self.count += 1  # [attribute-defined-outside-init]


# Should not trigger: some attributes in __init__, others not (only flag the ones not in __init__)
class MixedAttributes:
    def __init__(self):
        self.initialized_attr = True

    def setup(self):
        self.initialized_attr = False  # This is fine
        self.new_attr = "new"  # [attribute-defined-outside-init]


# Should not trigger: class variable with type annotation
class AnnotatedClassVar:
    is_registered: bool = False

    def register(self):
        self.is_registered = True  # This is fine, it's a class variable


# Should not trigger: custom decorator
def custom_decorator(cls):
    return cls


@custom_decorator
class DecoratedClass:
    def setup(self):
        self.attr = "value"  # Should not trigger, class is decorated


# Complex inheritance case - should not trigger
class A:
    pass


class B(A):
    def setup(self):
        self.attr = "value"  # Should not trigger, has parent


# Empty class with inheritance - should not trigger
class EmptyChild(dict):
    def setup(self):
        self.attr = "value"  # Should not trigger, inherits from dict


# Should not trigger: setattr call (dynamic attribute setting)
class StudentWithSetattr:
    def __init__(self):
        pass

    def register(self):
        setattr(self, "is_registered", True)  # Should not trigger, uses setattr function call


# Should not trigger: property setter
class StudentWithProperty:
    @property
    def name(self):
        return self._name

    @name.setter
    def name(self, value):
        self._name = value  # Should not trigger, assignment in property setter 
