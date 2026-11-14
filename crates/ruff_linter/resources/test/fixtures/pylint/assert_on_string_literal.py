def test_division():
    a = 9 / 3
    assert "No ZeroDivisionError were raised"  # [assert-on-string-literal]


def test_division():
    a = 9 / 3
    assert a == 3


try:
    assert "bad"  # [assert-on-string-literal]
except:
    assert "bad again"  # [assert-on-string-literal]

a = 12
assert f"hello {a}"  # [assert-on-string-literal]
assert f"{a}"  # [assert-on-string-literal]
assert f""  # [assert-on-string-literal]
assert ""  # [assert-on-string-literal]
assert b"hello"  # [assert-on-string-literal]
assert "", b"hi"  # [assert-on-string-literal]
assert "WhyNotHere?", "HereIsOk"  # [assert-on-string-literal]
assert 12, "ok here"

assert "active" if condition else item.status  # [assert-on-string-literal]
assert item.status if condition else "inactive"  # [assert-on-string-literal]
assert "" if condition else item.status  # [assert-on-string-literal]
assert item.status if condition else ""  # [assert-on-string-literal]
assert item.status == ("active" if condition else "inactive")


# t-strings are always True even when "empty"
# skip lint in this case
assert t""
assert t"hey"
assert t"{a}"
