# Mock objects
# ============
# Errors
assert my_mock.not_called()
assert my_mock.called_once_with()
assert my_mock.not_called
assert my_mock.called_once_with
my_mock.assert_not_called
my_mock.assert_called
my_mock.assert_called_once_with
my_mock.assert_called_once_with
MyMock.assert_called_once_with

# OK
assert my_mock.call_count == 1
assert my_mock.called
my_mock.assert_not_called()
my_mock.assert_called()
my_mock.assert_called_once_with()
"""like :meth:`Mock.assert_called_once_with`"""
"""like :meth:`MagicMock.assert_called_once_with`"""

# AsyncMock objects
# =================
# Errors
assert my_mock.not_awaited()
assert my_mock.awaited_once_with()
assert my_mock.not_awaited
assert my_mock.awaited_once_with
my_mock.assert_not_awaited
my_mock.assert_awaited
my_mock.assert_awaited_once_with
my_mock.assert_awaited_once_with
MyMock.assert_awaited_once_with
assert my_mock.awaited

# OK
assert my_mock.await_count == 1
my_mock.assert_not_awaited()
my_mock.assert_awaited()
my_mock.assert_awaited_once_with()
"""like :meth:`Mock.assert_awaited_once_with`"""
"""like :meth:`MagicMock.assert_awaited_once_with`"""
