class Str:
    def __str__(self):
        return 1

class Float:
    def __str__(self):
        return 3.05
    
class Int:
    def __str__(self):
        return 0
    
class Bool:
    def __str__(self):
        return False
    
class Str2:
    def __str__(self):
        x = "ruff"
        return x
    
# TODO fixme once Ruff has better type checking
def return_int():
    return 3

class ComplexReturn:
    def __str__(self):
        return return_int()