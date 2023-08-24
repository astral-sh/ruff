assert True  # Trailing same-line
assert True is True  # Trailing same-line
assert 1, "Some string"  # Trailing same-line
assert aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  # Trailing same-line

assert (  # Dangle1
    # Dangle2
)

# TODO: https://github.com/astral-sh/ruff/pull/5168#issuecomment-1630767421
# Leading assert
assert (
    # Leading test value
    True  # Trailing test value same-line
    # Trailing test value own-line
), "Some string"  # Trailing msg same-line
# Trailing assert

# Random dangler

# TODO: https://github.com/astral-sh/ruff/pull/5168#issuecomment-1630767421
# Leading assert
assert (
    # Leading test value
    True  # Trailing test value same-line
    # Trailing test value own-line

    # Test dangler
), "Some string"  # Trailing msg same-line
# Trailing assert

def test():
    assert {
        key1: value1,
        key2: value2,
        key3: value3,
        key4: value4,
        key5: value5,
        key6: value6,
        key7: value7,
        key8: value8,
        key9: value9,
    } == expected, (
        "Not what we expected and the message is too long to fit ineeeeee one line"
    )

    assert {
        key1: value1,
        key2: value2,
        key3: value3,
        key4: value4,
        key5: value5,
        key6: value6,
        key7: value7,
        key8: value8,
        key9: value9,
    } == expected, (
        "Not what we expected and the message is too long to fit in one lineeeee"
    )

    assert {
        key1: value1,
        key2: value2,
        key3: value3,
        key4: value4,
        key5: value5,
        key6: value6,
        key7: value7,
        key8: value8,
        key9: value9,
    } == expected, "Not what we expected and the message is too long to fit in one lineeeeeeeeeeeee"

    assert (
        {
            key1: value1,
            key2: value2,
            key3: value3,
            key4: value4,
            key5: value5,
            key6: value6,
            key7: value7,
            key8: value8,
            key9: value9,
        }
        == expectedeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
    ), "Not what we expected and the message is too long to fit in one lin"

    assert (
        {
            key1: value1,
            key2: value2,
            key3: value3,
            key4: value4,
            key5: value5,
            key6: value6,
            key7: value7,
            key8: value8,
            key9: value9,
        }
        == expectedeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
    ), "Not what we expected and the message is too long to fit in one lineeeeeeeeeeeeeeeee"

    assert expected == {
        key1: value1,
        key2: value2,
        key3: value3,
        key4: value4,
        key5: value5,
        key6: value6,
        key7: value7,
        key8: value8,
        key9: value9,
    }, "Not what we expected and the message is too long to fit ineeeeee one line"

    assert expected == {
        key1: value1,
        key2: value2,
        key3: value3,
        key4: value4,
        key5: value5,
        key6: value6,
        key7: value7,
        key8: value8,
        key9: value9,
    }, "Not what we expected and the message is too long to fit in one lineeeeeeeeeeeeeeeeeeee"

    assert (
        expectedeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
        == {
            key1: value1,
            key2: value2,
            key3: value3,
            key4: value4,
            key5: value5,
            key6: value6,
            key7: value7,
            key8: value8,
            key9: value9,
        }
    ), "Not what we expected and the message is too long to fit in one lin"

    assert (
        expectedeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
        == {
            key1: value1,
            key2: value2,
            key3: value3,
            key4: value4,
            key5: value5,
            key6: value6,
            key7: value7,
            key8: value8,
            key9: value9,
        }
    ), "Not what we expected and the message is too long to fit in one lineeeeeeeeeeeeeee"
