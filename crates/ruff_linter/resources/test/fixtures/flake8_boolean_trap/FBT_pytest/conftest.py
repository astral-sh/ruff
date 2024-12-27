# Errors

def this_is_not_a_hook(a: bool): ...


# No errors

def pytest_this_is_a_pytest_hook(a: bool): ...
