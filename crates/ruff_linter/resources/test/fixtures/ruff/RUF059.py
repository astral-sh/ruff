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


### No errors

def outer_100(): ...


def outer_101(f): ...


def outer_102(yf): ...


def outer_103():
    outer_103 = 0


def outer_104():
    def inner_104[outer_104](): ...
