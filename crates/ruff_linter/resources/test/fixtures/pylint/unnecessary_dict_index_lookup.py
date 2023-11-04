FRUITS = {"apple": 1, "orange": 10, "berry": 22}

def fix_these():
    [FRUITS[fruit_name] for fruit_name, fruit_count in FRUITS.items()]  # PLR1733
    {FRUITS[fruit_name] for fruit_name, fruit_count in FRUITS.items()}  # PLR1733
    {fruit_name: FRUITS[fruit_name] for fruit_name, fruit_count in FRUITS.items()}  # PLR1733

    for fruit_name, fruit_count in FRUITS.items():
        print(FRUITS[fruit_name])  # PLR1733
        blah = FRUITS[fruit_name]  # PLR1733
        assert FRUITS[fruit_name] == "pear"  # PLR1733


def dont_fix_these():
    # once there is an assignment to the dict[index], we stop emitting diagnostics
    for fruit_name, fruit_count in FRUITS.items():
        FRUITS[fruit_name] = 0  # Ok
        assert FRUITS[fruit_name] == 0  # Ok


def value_intentionally_unused():
    [FRUITS[fruit_name] for fruit_name, _ in FRUITS.items()]  # PLR1733
    {FRUITS[fruit_name] for fruit_name, _ in FRUITS.items()}  # PLR1733
    {fruit_name: FRUITS[fruit_name] for fruit_name, _ in FRUITS.items()}  # PLR1733

    for fruit_name, _ in FRUITS.items():
        print(FRUITS[fruit_name])  # Ok
        blah = FRUITS[fruit_name]  # Ok
        assert FRUITS[fruit_name] == "pear"  # Ok
