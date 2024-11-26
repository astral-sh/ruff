import math  # Import statements can appear at the top of a file or within a class

class ExampleClass:
    """A class demonstrating all kinds of statements."""
    
    # Class attributes (class variables)
    class_var1 = "class-level attribute"
    class_var2 = 42
    
    # Instance attributes will be defined in the constructor
    def __init__(self, instance_var):
        """Constructor to initialize instance attributes."""
        self.instance_var = instance_var  # Instance variable
        self.computed_var = self._helper_method()  # Using a helper method in initialization
    
    # Instance method
    def instance_method(self):
        """Regular instance method."""
        print("This is an instance method.")
        self._private_method()
    
    # Private method (conventionally prefixed with an underscore)
    def _private_method(self):
        """A private helper method."""
        print("This is a private method.")

    # Protected method (by convention, a single leading underscore indicates protected)
    def _protected_method(self):
        print("This is a protected method.")
    
    # Static method
    @staticmethod
    def static_method():
        """A static method."""
        print("Static method called. No access to instance or class data.")
    
    # Class method
    @classmethod
    def class_method(cls):
        """A class method."""
        print(f"Class method called. class_var1 = {cls.class_var1}")
    
    # Special method (dunder methods)
    def __str__(self):
        """Special method for string representation."""
        return f"ExampleClass(instance_var={self.instance_var}, computed_var={self.computed_var})"
    
    def __len__(self):
        """Special method to define the behavior of len()."""
        return len(self.instance_var)
    
    # Nested class
    class NestedClass:
        """A class defined within another class."""
        def nested_method(self):
            print("Method of a nested class.")
    
    # Pass statement (used as a placeholder)
    def placeholder_method(self):
        """A method with no implementation yet."""
        pass
    
    # Try/Except block inside a method
    def error_handling_method(self):
        """A method with error handling."""
        try:
            result = 10 / 0  # Intentional error
        except ZeroDivisionError as e:
            print(f"Caught an exception: {e}")
        finally:
            print("Cleanup actions can be placed here.")
    
    # Using a decorator
    @property
    def readonly_property(self):
        """A read-only property."""
        return self.computed_var
    
    @readonly_property.setter
    def readonly_property(self, value):
        """Attempt to set this property raises an error."""
        raise AttributeError("This property is read-only.")
    
    # Conditional logic inside the class (not recommended but valid)
    if math.pi > 3:
        def conditionally_defined_method(self):
            print("This method exists because math.pi > 3.")
    
    # For loop inside the class (unusual but valid)
    for i in range(3):
        exec(f"def loop_method_{i}(self): print('Loop-defined method {i}')")
    
    # Docstrings for the class
    """
    Additional class-level comments can be included in the docstring.
    """
    
    # List comprehensions inside a class (valid but rarely used)
    squares = [x ** 2 for x in range(5)]
    
    # Lambda functions inside a class (unusual but valid)
    double = lambda x: x * 2


g = 'module attribute (module-global variable)'
"""This is g's docstring."""

class AClass:

    c = 'class attribute'
    """This is AClass.c's docstring."""

    def __init__(self):
        """Method __init__'s docstring."""

        self.i = 'instance attribute'
        """This is self.i's docstring."""

class BClass:
    ...

class CClass:
    pass

class DClass:
    """Empty class."""

def f(x):
    """Function f's docstring."""
    return x**2

f.a = 1
"""Function attribute f.a's docstring."""
