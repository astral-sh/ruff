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
        letters[index] = "d"  # Ok
        letters[index] += "e"  # Ok
        assert letters[index] == "de"  # Ok
    
    # once there is an assignment to the index, we stop emitting diagnostics
    for index, letter in enumerate(letters):
        index += 1  # Ok
        print(letters[index])  # Ok
    
    # once there is an assignment to the sequence, we stop emitting diagnostics
    for index, letter in enumerate(letters):
        letters = ["d", "e", "f"]  # Ok
        print(letters[index])  # Ok

    # once there is an deletion from or of the sequence or index, we stop emitting diagnostics
    for index, letter in enumerate(letters):
        del letters[index]  # Ok
        print(letters[index])  # Ok
    for index, letter in enumerate(letters):
        del letters  # Ok
        print(letters[index])  # Ok
    for index, letter in enumerate(letters):
        del index  # Ok
        print(letters[index])  # Ok


def value_intentionally_unused():
    [letters[index] for index, _ in enumerate(letters)]  # PLR1736
    {letters[index] for index, _ in enumerate(letters)}  # PLR1736
    {index: letters[index] for index, _ in enumerate(letters)}  # PLR1736

    for index, _ in enumerate(letters):
        print(letters[index])  # Ok
        blah = letters[index]  # Ok
        letters[index] = "d"  # Ok
