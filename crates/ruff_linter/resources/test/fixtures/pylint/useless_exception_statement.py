
# Violation test cases: PLW0133

# Test case 1: Useless exception statement
from abc import ABC, abstractmethod
from contextlib import suppress


def test():
    AssertionError("This is an assertion error")  # PLW0133

# Test case 2: Useless exception statement in try-except block

def test():
    try:
        Exception("This is an exception") # PLW0133
    except Exception as e:
        pass  # PL

# Test case 3: Useless exception statement in if statement

def test():
    if True:
        RuntimeError("This is an exception")  # PLW0133

# Test case 4: Useless exception statement in class

def test():
    class Test:
        def __init__(self):
            TypeError("This is an exception") # PLW0133

# Test case 5: Useless exception statement in function

def test():
    def f():
        IndexError("This is an exception") # PLW0133
    f()

# Test case 6: Useless exception statement in while loop
    
def test():
    while True:
        KeyError("This is an exception") # PLW0133

# Test case 7: Useless exception statement in abstract class
        
def test():
    class Test(ABC):
        @abstractmethod
        def test(self):
            NotImplementedError("This is an exception") # PLW0133


# Test case 8: Useless exception statement inside context manager
            
def test():
    with suppress(AttributeError):
        AttributeError("This is an exception") # PLW0133


# Non-violation test cases: PLW0133
            
# Test case 1: Used exception statement in try-except block

def test():
    raise Exception("This is an exception")  # OK

# Test case 2: Used exception statement in if statement

def test():
    if True:
        raise ValueError("This is an exception")  # OK
    
# Test case 3: Used exception statement in class
    
def test():
    class Test:
        def __init__(self):
            raise TypeError("This is an exception")  # OK
        
# Test case 4: Exception statement used in list comprehension

def test():
    [ValueError("This is an exception") for i in range(10)]  # OK


# Test case 5: Exception statement used when initializing a dictionary
            
def test():
    {i: TypeError("This is an exception") for i in range(10)}  # OK

# Test case 6: Exception statement used in function
    
def test():
    def f():
        raise IndexError("This is an exception")  # OK
    f()

# Test case 7: Exception statement used in variable assignment

def test():
    e = KeyError("This is an exception")  # OK


# Test case 8: Exception statement inside context manager
            
def test():
    with suppress(AttributeError):
        raise AttributeError("This is an exception") # OK