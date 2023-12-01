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
