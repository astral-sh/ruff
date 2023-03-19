def test_division():
    a = 9 / 3
    assert "No ZeroDivisionError were raised"  # [assert-on-string-literal]


def test_division():
    a = 9 / 3
    assert a == 3


try:
    assert "bad"
except:
    assert "bad again"

a = 12
assert f"hello {a}"
assert ""
assert b"hello"
assert "", b"hi"
assert "WhyNotHere?", "HereIsOk"  # [assert-on-string-literal]
assert 12, "ok here"
