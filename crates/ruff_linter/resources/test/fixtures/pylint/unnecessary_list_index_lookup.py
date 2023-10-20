letters = ["a", "b", "c"]


def fix_these():
    [letters[index] for index, letter in enumerate(letters)]  # PLR1736
    {letters[index] for index, letter in enumerate(letters)}  # PLR1736
    {letter: letters[index] for index, letter in enumerate(letters)}  # PLR1736

    for index, letter in enumerate(letters):
        print(letters[index])  # PLR1736
        blah = letters[index]  # PLR1736
        letters[index]: str = letters[index]  # PLR1736 (on the right hand)
        letters[index] += letters[index]  # PLR1736 (on the right hand)
        letters[index] = letters[index]  # PLR1736 (on the right hand)


def dont_fix_these():
    for index, letter in enumerate(letters):
        letters[index] = "d"  # Ok


def value_intentionally_unused():
    [letters[index] for index, _ in enumerate(letters)]  # PLR1736
    {letters[index] for index, _ in enumerate(letters)}  # PLR1736
    {index: letters[index] for index, _ in enumerate(letters)}  # PLR1736

    for index, _ in enumerate(letters):
        print(letters[index])  # Ok
        blah = letters[index]  # Ok
        letters[index]: str = letters[index]  # Ok
        letters[index] += letters[index]  # Ok
        letters[index] = letters[index]  # Ok
        letters[index] = "d"  # Ok
