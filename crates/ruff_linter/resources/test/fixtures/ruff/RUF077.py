# Variables with non-ASCII names
переменная = 42  # Cyrillic variable name
variable_ascii = 42  # OK

# Non-ASCII attribute access
class Bar:
    метод = "hello"

bar = Bar()
bar.метод  # Non-ASCII attribute access

# Function with non-ASCII parameter
def func(параметр):
    pass

# Non-ASCII function name
def функция():
    pass

# Non-ASCII class name
class Класс:
    pass

# Multiple non-ASCII scripts
значение_変数 = "test"  # Mix of Cyrillic and CJK

# ASCII names (should NOT trigger)
hello = "world"
obj.attr

def normal_func(normal_arg):
    pass

class NormalClass:
    pass
