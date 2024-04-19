# These testcases should raise errors


class Bool:
    def __hash__(self):
        return True  # [invalid-hash-return]


class Float:
    def __hash__(self):
        return 3.05  # [invalid-hash-return]


class Str:
    def __hash__(self):
        return "ruff"  # [invalid-hash-return]


class HashNoReturn:
    def __hash__(self):
        print("ruff")  # [invalid-hash-return]


# TODO: Once Ruff has better type checking
def return_int():
    return "3"


class ComplexReturn:
    def __hash__(self):
        return return_int()  # [invalid-hash-return]


# These testcases should NOT raise errors


class Hash:
    def __hash__(self):
        return 7741


class Hash2:
    def __hash__(self):
        x = 7741
        return x


class Hash3:
    def __hash__(self): ...


class Has4:
    def __hash__(self):
        pass


class Hash5:
    def __hash__(self):
        raise NotImplementedError


class HashWrong6:
    def __hash__(self):
        print("raise some error")
        raise NotImplementedError
