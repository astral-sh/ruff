FRUITS = {"apple": 1, "orange": 10, "berry": 22}

def fix_these():
    for fruit_name, fruit_count in FRUITS.items():
        print(FRUITS[fruit_name])  # PLR1733
        blah = FRUITS[fruit_name]  # PLR1733
        FRUITS[fruit_name] = FRUITS[fruit_name]  # PLR1733 (on the right hand)


def dont_fix_these():
    for fruit_name, fruit_count in FRUITS.items():
        FRUITS[fruit_name] = 0  # Ok


def value_intentionally_unused():
    for fruit_name, _ in FRUITS.items():
        print(FRUITS[fruit_name])  # Ok
        blah = FRUITS[fruit_name]  # Ok
        FRUITS[fruit_name] = FRUITS[fruit_name]  # Ok
        FRUITS[fruit_name] = "d"  # Ok
