assert True  # Trailing same-line
assert True is True  # Trailing same-line
assert 1, "Some string"  # Trailing same-line
assert aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  # Trailing same-line

assert (  # Dangle1
    # Dangle2
)

# TODO: https://github.com/astral-sh/ruff/pull/5168#issuecomment-1630767421
# Leading assert
assert (
    # Leading test value
    True  # Trailing test value same-line
    # Trailing test value own-line 
), "Some string"  # Trailing msg same-line
# Trailing assert

# Random dangler

# TODO: https://github.com/astral-sh/ruff/pull/5168#issuecomment-1630767421
# Leading assert
assert (
    # Leading test value
    True  # Trailing test value same-line
    # Trailing test value own-line 

    # Test dangler
), "Some string"  # Trailing msg same-line
# Trailing assert
