import abc
from abc import abstractmethod

import pytest


@pytest.fixture()
def my_fixture(mocker):  # OK with return
    return 0


@pytest.fixture()
def activate_context():  # OK with yield
    with get_context() as context:
        yield context


@pytest.fixture()
def _any_fixture(mocker):  # Ok nested function
    def nested_function():
        return 1

    mocker.patch("...", nested_function)


class BaseTest:
    @pytest.fixture()
    @abc.abstractmethod
    def _my_fixture():  # OK abstract with import abc
        return NotImplemented


class BaseTest:
    @pytest.fixture()
    @abstractmethod
    def _my_fixture():  # OK abstract with from import
        return NotImplemented


@pytest.fixture()
def _my_fixture(mocker):  # Error with return
    return 0


@pytest.fixture()
def _activate_context():  # Error with yield
    with get_context() as context:
        yield context


@pytest.fixture()
def _activate_context():  # Error with conditional yield from
    if some_condition:
        with get_context() as context:
            yield context
    else:
        yield from other_context()
