# These testcases should raise errors

class Float:
    def __bool__(self):
        return 3.05  # [invalid-bool-return]

class Int:
    def __bool__(self):
        return 0  # [invalid-bool-return]


class Str:
    def __bool__(self):
        x = "ruff"
        return x  # [invalid-bool-return]

# TODO: Once Ruff has better type checking
def return_int():
    return 3

class ComplexReturn:
    def __bool__(self):
        return return_int()  # [invalid-bool-return]



# These testcases should NOT raise errors

class Bool:
    def __bool__(self):
        return True


class Bool2:
    def __bool__(self):
        x = True
        return x