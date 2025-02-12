import pytest

@pytest.fixture
def f(): ...

@pytest.yield_fixture
def yf(): ...


### Errors

global_a = 1

def outer_1(a):
    def inner_1(a): ...


def outer_2():
    a = 1
    def inner_2(a): ...


def outer_3():
    def inner_3(a): ...
    a = 1


def outer_4():
    def inner_4(outer_4): ...


def outer_5(global_a): ...


def outer_6():
    def inner_6(global_a): ...


def outer_7():
    def inner_7(f): ...


def outer_8():
    def inner_8(yf): ...


def outer_9(outer_100):
    ...


def outer_10():
    def inner_10(outer_100): ...


class Outer11[T]:
    def inner_11(self, T): ...


def outer_12():
    a = 0
    _ = lambda a: a + 1


def outer_13():
    _ = lambda a: lambda a, b: a + b - 1


def outer_14():
    _ = (lambda a, b: a - b / 2 for a, b in [])


### No errors

def outer_100(): ...


def outer_101(f): ...


def outer_102(yf): ...


def outer_103():
    outer_103 = 0


def outer_104():
    def inner_104[outer_104](): ...


class Outer105:
    a = 1
    def inner_105(self, a): ...


class Outer106:
    def inner_105(self, a): ...
    a = 1


class Outer107:
    def inner_105(self, a): ...
    def a(self): ...


class Outer108:
    _ = lambda a: a + 1
    a = 0


class Outer109:
    a = 0
    _ = [lambda a: a @ 3 for _ in [1]]
