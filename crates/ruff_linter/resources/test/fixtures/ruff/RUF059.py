import pytest

@pytest.fixture
def f(): ...

@pytest.yield_fixture
def yf(): ...


### Errors

global_a = 1

def outer(a):
    def inner(a): ...


def outer():
    a = 1
    def inner(a): ...


def outer():
    def inner(a): ...
    a = 1


def outer():
    def inner(outer): ...


def outer(global_a): ...


def outer():
    def inner(global_a): ...


def outer():
    def inner(f): ...


def outer():
    def inner(yf): ...


### No errors


def outer(f): ...


def outer(yf): ...
