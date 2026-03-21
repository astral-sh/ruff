# Regression test for https://github.com/astral-sh/ruff/pull/20962
# error message:
# `place_by_id: execute: too many cycle iterations`

name_5(name_3)
[0 for unique_name_0 in unique_name_1 for unique_name_2 in name_3]

@{name_3 for unique_name_3 in unique_name_4}
class name_4[**name_3](0, name_2=name_5):
    pass

try:
    name_0 = name_4
except* 0:
    pass
else:
    match unique_name_12:
        case 0:
            from name_2 import name_3
        case name_0():

            @name_4
            def name_3():
                pass

(name_3 := 0)

@name_3
async def name_5():
    pass
