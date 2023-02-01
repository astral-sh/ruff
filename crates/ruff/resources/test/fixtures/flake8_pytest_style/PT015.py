import pytest


def test_ok():
    assert [0]


def test_error():
    assert None
    assert False
    assert 0
    assert 0.0
    assert ""
    assert f""
    assert []
    assert ()
    assert {}
    assert list()
    assert set()
    assert tuple()
    assert dict()
    assert frozenset()
    assert list([])
    assert set(set())
    assert tuple("")
