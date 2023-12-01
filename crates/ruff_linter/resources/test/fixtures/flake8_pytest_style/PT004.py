import abc
from abc import abstractmethod

import pytest


@pytest.fixture()
def _patch_something(mocker):  # OK simple
    mocker.patch("some.thing")


@pytest.fixture()
def _patch_something(mocker):  # OK with return
    if something:
        return
    mocker.patch("some.thing")


@pytest.fixture()
def _activate_context():  # OK with yield
    with context:
        yield


class BaseTest:
    @pytest.fixture()
    @abc.abstractmethod
    def my_fixture():  # OK abstract with import abc
        raise NotImplementedError


class BaseTest:
    @pytest.fixture()
    @abstractmethod
    def my_fixture():  # OK abstract with from import
        raise NotImplementedError


@pytest.fixture()
def my_fixture():  # OK ignoring yield from
    yield from some_generator()



@pytest.fixture()
def my_fixture():  # OK ignoring yield value
    yield 1


@pytest.fixture()
def patch_something(mocker):  # Error simple
    mocker.patch("some.thing")


@pytest.fixture()
def activate_context():  # Error with yield
    with context:
        yield
