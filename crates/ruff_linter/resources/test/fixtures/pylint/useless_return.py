import sys


def print_python_version():
    print(sys.version)
    return None  # [useless-return]


def print_python_version():
    print(sys.version)
    return None  # [useless-return]


def print_python_version():
    print(sys.version)
    return None  # [useless-return]


class SomeClass:
    def print_python_version(self):
        print(sys.version)
        return None  # [useless-return]


def print_python_version():
    if 2 * 2 == 4:
        return
    print(sys.version)


def print_python_version():
    if 2 * 2 == 4:
        return None
    return


def print_python_version():
    if 2 * 2 == 4:
        return None


def print_python_version():
    """This function returns None."""
    return None


def print_python_version():
    """This function returns None."""
    print(sys.version)
    return None  # [useless-return]


class BaseCache:
    def get(self, key: str) -> str | None:
        print(f"{key} not found")
        return None

    def get(self, key: str) -> None:
        print(f"{key} not found")
        return None


# https://github.com/astral-sh/ruff/issues/28861
def print_python_version():
    print(sys.version)

    return  # trailing comment is preserved


class SomeClass:
    def print_python_version(self):
        print(sys.version)
        return None  # trailing comment is preserved


def print_python_version():
    print(sys.version)
    return; # comment after a semicolon is preserved


def print_python_version():
    print(sys.version); return  # comment after a same-line `return` is preserved


def print_python_version():
    print(sys.version)
    return (  # comment inside the `return` makes the fix unsafe
        None
    )
