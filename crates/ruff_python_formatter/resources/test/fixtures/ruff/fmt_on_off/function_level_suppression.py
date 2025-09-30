# Test case 1: Function-level fmt: off should not affect module-level formatting
def test_function():
    # fmt: off
    print("hello")
    print("world")
    # fmt: on
    print("formatted")


# This should be formatted normally at module level
print("module level")
x = 1 + 2
y = 3 + 4
print(x + y)


# Test case 2: Multiple functions with fmt: off should work independently
def another_function():
    # fmt: off
    a = 1 + 2
    b = 3 + 4
    # fmt: on
    return a + b


# More module-level code that should be formatted
def normal_function():
    return "normal formatting"


# Test case 3: Function ending with fmt: off should not affect trailing newline
def function_with_trailing_suppression():
    # fmt: off
    print("suppressed")
