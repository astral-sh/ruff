# Regression tests for https://github.com/astral-sh/ruff/issues/11569


# comment 1
def foo(self) -> None: ...
def bar(self) -> None: ...
# comment 2

# comment 3
def baz(self) -> None:
    return None
# comment 4


def foo(self) -> None: ...
# comment 5

def baz(self) -> None:
    return None


def foo(self) -> None:
    ... # comment 5
def baz(self) -> None:
    return None

def foo(self) -> None: ...
# comment 5
