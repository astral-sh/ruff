# These testcases should raise errors


class Bool:
    """pylint would not raise, but ruff does - see explanation in the docs"""

    def __index__(self):
        return True  # [invalid-index-return]


class Float:
    def __index__(self):
        return 3.05  # [invalid-index-return]


class Dict:
    def __index__(self):
        return {"1": "1"}  # [invalid-index-return]


class Str:
    def __index__(self):
        return "ruff"  # [invalid-index-return]


class IndexNoReturn:
    def __index__(self):
        print("ruff")  # [invalid-index-return]


# TODO: Once Ruff has better type checking
def return_index():
    return "3"


class ComplexReturn:
    def __index__(self):
        return return_index()  # [invalid-index-return]


# These testcases should NOT raise errors


class Index:
    def __index__(self):
        return 0


class Index2:
    def __index__(self):
        x = 1
        return x


class Index3:
    def __index__(self):
        ...


class Index4:
    def __index__(self):
        pass


class Index5:
    def __index__(self):
        raise NotImplementedError


class Index6:
    def __index__(self):
        print("raise some error")
        raise NotImplementedError
