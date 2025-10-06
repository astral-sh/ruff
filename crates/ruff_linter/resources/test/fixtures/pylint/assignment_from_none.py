"""Test cases for assignment-from-none rule."""

# Test assignment from functions that return None
def function_returns_none():
    return None

x = function_returns_none()  # [assignment-from-none]

# Test assignment from print function
result = print("Hello")  # [assignment-from-none]

# Test assignment from sys.exit
import sys
exit_code = sys.exit(0)  # [assignment-from-none]

# Test assignment from list.sort
my_list = [1, 2, 3]
sorted_result = my_list.sort()  # [assignment-from-none]

# Test assignment from file.close
with open("test.txt", "w") as f:
    close_result = f.close()  # [assignment-from-none]

# Test annotated assignment
value: int = function_returns_none()  # [assignment-from-none]

# Test multiple assignment (should not trigger)
a, b = 1, 2

# Test assignment from function that doesn't return None
def function_returns_value():
    return 42

good_value = function_returns_value()

# Test assignment from literal (should not trigger)
literal_value = None

# Test assignment from complex expression (should not trigger)
complex_value = function_returns_none() or 42

# Test assignment from time.sleep
import time
sleep_result = time.sleep(1)  # [assignment-from-none]

# Test assignment from random.seed
import random
seed_result = random.seed(42)  # [assignment-from-none]

# Test assignment from dict.clear
my_dict = {"a": 1, "b": 2}
clear_result = my_dict.clear()  # [assignment-from-none]

# Test assignment from set.clear  
my_set = {1, 2, 3}
set_clear_result = my_set.clear()  # [assignment-from-none]

# Test assignment from list.reverse
my_list = [3, 2, 1]
reverse_result = my_list.reverse()  # [assignment-from-none]

# Test assignment from quit
quit_result = quit()  # [assignment-from-none]

# Test assignment from os._exit
import os
os_exit_result = os._exit(0)  # [assignment-from-none]

# Test assignment from function that returns value (should not trigger)
def returns_int():
    return 42

int_result = returns_int()

# Test assignment from function with conditional return (should not trigger)
def maybe_none():
    if True:
        return 42
    return None

maybe_result = maybe_none()

# Test assignment to attribute (should not trigger - not a simple name)
class MyClass:
    pass

obj = MyClass()
obj.attr = function_returns_none()

# Test assignment in tuple unpacking (should not trigger)
a, b = function_returns_none(), 42

# Test assignment to subscript (should not trigger)
my_list = [None]
my_list[0] = function_returns_none()