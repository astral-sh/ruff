import builtins

letters = ["a", "b", "c"]


def fix_these():
    [letters[index] for index, letter in enumerate(letters)]  # PLR1736
    {letters[index] for index, letter in enumerate(letters)}  # PLR1736
    {letter: letters[index] for index, letter in enumerate(letters)}  # PLR1736

    for index, letter in enumerate(letters):
        print(letters[index])  # PLR1736
        blah = letters[index]  # PLR1736
        assert letters[index]  == "d"  # PLR1736

    for index, letter in builtins.enumerate(letters):
        print(letters[index])  # PLR1736
        blah = letters[index]  # PLR1736
        assert letters[index]  == "d"  # PLR1736


def dont_fix_these():
    # once there is an assignment to the sequence[index], we stop emitting diagnostics
    for index, letter in enumerate(letters):
        letters[index] = "d"  # OK
        letters[index] += "e"  # OK
        assert letters[index] == "de"  # OK

    # once there is an assignment to the index, we stop emitting diagnostics
    for index, letter in enumerate(letters):
        index += 1  # OK
        print(letters[index])  # OK

    # once there is an assignment to the sequence, we stop emitting diagnostics
    for index, letter in enumerate(letters):
        letters = ["d", "e", "f"]  # OK
        print(letters[index])  # OK

    # once there is an assignment to the value, we stop emitting diagnostics
    for index, letter in enumerate(letters):
        letter = "d"
        print(letters[index])  # OK

    # once there is an deletion from or of the sequence or index, we stop emitting diagnostics
    for index, letter in enumerate(letters):
        del letters[index]  # OK
        print(letters[index])  # OK
    for index, letter in enumerate(letters):
        del letters  # OK
        print(letters[index])  # OK
    for index, letter in enumerate(letters):
        del index  # OK
        print(letters[index])  # OK


def value_intentionally_unused():
    [letters[index] for index, _ in enumerate(letters)]  # OK
    {letters[index] for index, _ in enumerate(letters)}  # OK
    {index: letters[index] for index, _ in enumerate(letters)}  # OK

    for index, _ in enumerate(letters):
        print(letters[index])  # OK
        blah = letters[index]  # OK
        letters[index] = "d"  # OK
