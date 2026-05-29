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


def start():
    # OK
    for index, list_item in enumerate(some_list, start=1):
        print(some_list[index])

    # PLR1736
    for index, list_item in enumerate(some_list, start=0):
        print(some_list[index])

    # PLR1736
    for index, list_item in enumerate(some_list):
        print(some_list[index])


def nested_index_lookup():
    data = {"a": 1, "b": 2}
    column_names = ["a", "b"]
    for index, column_name in enumerate(column_names):
        _ = data[column_names[index]]  # PLR1736


def for_else_no_false_positive(letters):
    # The `else` branch runs even when the loop body never executes (empty
    # iterable), so `index` and `letter` may be unbound there. No PLR1736.
    for index, letter in enumerate(letters):
        if letter == "z":
            break
    else:
        print(letters[index])  # OK - index may be unbound


def inner_loop_shadowing(letters):
    # `index` from the outer loop is shadowed by the inner `for index in ...`.
    # The lookup `letters[index]` inside the inner loop refers to the inner
    # `index`, not the outer one, so it must NOT be flagged.
    for index, letter in enumerate(letters):
        for index in range(3):       # shadows outer `index`
            print(letters[index])    # OK - not the same `index`
        for letter in range(3):      # shadows outer `letter`
            print(letters[index])    # OK - not the same `letter`
        # After an inner loop rebinds `index`, the visitor conservatively stops
        # flagging for the rest of the outer body (modified=true).
        print(letters[index])        # OK - conservative: outer `index` is gone
