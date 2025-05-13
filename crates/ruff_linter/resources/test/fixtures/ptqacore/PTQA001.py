import allure

# Examples for rule PTQA001: test_* functions must have a decorator @allure.id("__ЧИСЛО__")

def test_no_decorator():  # PTQA001
    assert True

@allure.id("123")          # OK
def test_with_numeric_id():
    assert True

@allure.id("abc")          # PTQA001: id is not purely numeric
def test_with_non_numeric_id():
    assert True

@allure.id("00123")        # OK: leading zeros are allowed
def test_with_leading_zeros():
    assert True

@allure.id("")             # PTQA001: empty string is invalid
def test_with_empty_id():
    assert True

@allure.id(" 456 ")        # PTQA001: whitespace around digits invalid
def test_with_whitespace_in_id():
    assert True

@allure.id("789")
@allure.tag("smoke")        # OK: extra decorators are fine
def test_with_multiple_decorators():
    assert True

@allure.tag("regression")   # PTQA001: missing @allure.id
def test_missing_id_but_has_other_decorator():
    assert True

@allure.id("42")
def not_a_test_function():  # OK: rule only applies to test_* functions
    assert True

def helper_function():      # OK: rule does not apply
    pass

@allure.id("314159")
def test_pi_value():       # OK
    assert True

@allure.id("3.14")         # PTQA001: contains non-integer characters
def test_with_decimal_id():
    assert True

@allure.id("100")
def test_underscore_in_name_ok():  # OK
    assert True

def test_another_missing_one():      # PTQA001
    pass
