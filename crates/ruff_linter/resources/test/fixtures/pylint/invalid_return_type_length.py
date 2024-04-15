# These testcases should raise errors

class Bool:
    def __len__(self):
        return True  # [invalid-length-return]

class Float:
    def __len__(self):
        return 3.05  # [invalid-length-return]

class Str:
    def __len__(self):
        return "ruff"  # [invalid-length-return]

class LengthNegative:
    def __len__(self):
        return -42  # [invalid-length-return]

# TODO: Once Ruff has better type checking
def return_int():
    return "3"

class ComplexReturn:
    def __len__(self):
        return return_int()  # [invalid-length-return]



# These testcases should NOT raise errors

class Length:
    def __len__(self):
        return 42
