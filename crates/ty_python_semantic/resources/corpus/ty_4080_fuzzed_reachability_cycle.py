# Regression test for https://github.com/astral-sh/ty/issues/4080
# Minimized from py-fuzzer seed 945. Prefix warming must not cause this cycle to diverge.

lambda: name_3

for name_0 in {lambda: name_0: 0}:
    pass
else:
    try:
        while name_0:
            pass
        unique_name_0()
    except* 0:
        pass
    finally:
        with 0 as name_0:
            pass

try:
    assert lambda: name_0
    unique_name_1()
except:
    while unique_name_2:
        pass
finally:
    import name_3

match 0:
    case {**name_0}:
        pass
