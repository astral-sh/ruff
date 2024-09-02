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
        FRUITS[fruit_name] = 0  # OK
        assert FRUITS[fruit_name] == 0  # OK

    # once there is an assignment to the key, we stop emitting diagnostics
    for fruit_name, fruit_count in FRUITS.items():
        fruit_name = 0  # OK
        assert FRUITS[fruit_name] == 0  # OK

    # once there is an assignment to the value, we stop emitting diagnostics
    for fruit_name, fruit_count in FRUITS.items():
        if fruit_count < 5:
            fruit_count = -fruit_count
        assert FRUITS[fruit_name] == 0  # OK


def value_intentionally_unused():
    [FRUITS[fruit_name] for fruit_name, _ in FRUITS.items()]  # OK
    {FRUITS[fruit_name] for fruit_name, _ in FRUITS.items()}  # OK
    {fruit_name: FRUITS[fruit_name] for fruit_name, _ in FRUITS.items()}  # OK

    for fruit_name, _ in FRUITS.items():
        print(FRUITS[fruit_name])  # OK
        blah = FRUITS[fruit_name]  # OK
        assert FRUITS[fruit_name] == "pear"  # OK


def rewrite_client_arrays(value_arrays: dict[str, list[int]]) -> dict[str, list[int]]:
    """Function from https://github.com/zulip/zulip/blob/3da91e951cd03cfa0b9c67378224e348353f36a6/analytics/views/stats.py#L617C1-L626C25"""
    mapped_arrays: dict[str, list[int]] = {}
    for label, array in value_arrays.items():
        mapped_label = client_label_map(label)
        if mapped_label in mapped_arrays:
            for i in range(len(array)):
                mapped_arrays[mapped_label][i] += value_arrays[label][i]  # PLR1733
        else:
            mapped_arrays[mapped_label] = [value_arrays[label][i] for i in range(len(array))]  # PLR1733
    return mapped_arrays
