# These testcases should raise errors


class Float:
    def __str__(self):
        return 3.05


class Int:
    def __str__(self):
        return 1


class Int2:
    def __str__(self):
        return 0


class Bool:
    def __str__(self):
        return False


# TODO: Once Ruff has better type checking
def return_int():
    return 3


class ComplexReturn:
    def __str__(self):
        return return_int()


# These testcases should NOT raise errors


class Str:
    def __str__(self):
        return "ruff"


class Str2:
    def __str__(self):
        x = "ruff"
        return x


class Str3:
    def __str__(self): ...


class Str4:
    def __str__(self):
        raise RuntimeError("__str__ not allowed")


class Str5:
    def __str__(self):  # PLE0307 (returns None if x <= 0)
        if x > 0:
            raise RuntimeError("__str__ not allowed")
