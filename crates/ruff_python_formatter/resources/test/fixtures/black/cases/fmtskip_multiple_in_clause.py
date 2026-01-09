# Multiple fmt: skip in multi-part if-clause
class ClassWithALongName:
    Constant1 = 1
    Constant2 = 2
    Constant3 = 3


def test():
    if (
        "cond1" == "cond1"
        and "cond2" == "cond2"
        and 1 in (
            ClassWithALongName.Constant1,
            ClassWithALongName.Constant2,
            ClassWithALongName.Constant3, # fmt: skip
        ) # fmt: skip
    ):
        return True
    return False
