import pytest
from pytest_mock import MockFixture

from module.sub import (
    get_value,
)


@pytest.fixture
def mock_check_output(mocker: MockFixture, output: bytes):
    return mocker.patch("subprocess.check_output", return_value=output)


@pytest.mark.parametrize(
    "output, expected_result",
    [(b"apple", "apple")],
)
def test_get_value(mock_check_output, expected_result):
    assert get_value() == expected_result
    mock_check_output.assert_called_once()
