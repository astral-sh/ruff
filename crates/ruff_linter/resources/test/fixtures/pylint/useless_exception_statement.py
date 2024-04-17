from abc import ABC, abstractmethod
from contextlib import suppress


# Test case 1: Useless exception statement
def func():
    AssertionError("This is an assertion error")  # PLW0133


# Test case 2: Useless exception statement in try-except block
def func():
    try:
        Exception("This is an exception")  # PLW0133
    except Exception as err:
        pass


# Test case 3: Useless exception statement in if statement
def func():
    if True:
        RuntimeError("This is an exception")  # PLW0133


# Test case 4: Useless exception statement in class
def func():
    class Class:
        def __init__(self):
            TypeError("This is an exception")  # PLW0133


# Test case 5: Useless exception statement in function
def func():
    def inner():
        IndexError("This is an exception")  # PLW0133

    inner()


# Test case 6: Useless exception statement in while loop
def func():
    while True:
        KeyError("This is an exception")  # PLW0133


# Test case 7: Useless exception statement in abstract class
def func():
    class Class(ABC):
        @abstractmethod
        def method(self):
            NotImplementedError("This is an exception")  # PLW0133


# Test case 8: Useless exception statement inside context manager
def func():
    with suppress(AttributeError):
        AttributeError("This is an exception")  # PLW0133


# Test case 9: Useless exception statement in parentheses
def func():
    (RuntimeError("This is an exception"))  # PLW0133


# Test case 10: Useless exception statement in continuation
def func():
    x = 1; (RuntimeError("This is an exception")); y = 2  # PLW0133


# Test case 11: Useless warning statement
def func():
    UserWarning("This is an assertion error")  # PLW0133


# Non-violation test cases: PLW0133


# Test case 1: Used exception statement in try-except block
def func():
    raise Exception("This is an exception")  # OK


# Test case 2: Used exception statement in if statement
def func():
    if True:
        raise ValueError("This is an exception")  # OK


# Test case 3: Used exception statement in class
def func():
    class Class:
        def __init__(self):
            raise TypeError("This is an exception")  # OK


# Test case 4: Exception statement used in list comprehension
def func():
    [ValueError("This is an exception") for i in range(10)]  # OK


# Test case 5: Exception statement used when initializing a dictionary
def func():
    {i: TypeError("This is an exception") for i in range(10)}  # OK


# Test case 6: Exception statement used in function
def func():
    def inner():
        raise IndexError("This is an exception")  # OK

    inner()


# Test case 7: Exception statement used in variable assignment
def func():
    err = KeyError("This is an exception")  # OK


# Test case 8: Exception statement inside context manager
def func():
    with suppress(AttributeError):
        raise AttributeError("This is an exception")  # OK
