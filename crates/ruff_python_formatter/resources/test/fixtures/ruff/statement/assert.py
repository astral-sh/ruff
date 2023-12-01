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

# Test for https://github.com/astral-sh/ruff/issues/7246
assert items == [
    "a very very very very very very very very very very very very very very very long string",
]

assert package.files == [
    {
        "file": "pytest-3.5.0-py2.py3-none-any.whl",
        "hash": "sha256:6266f87ab64692112e5477eba395cfedda53b1933ccd29478e671e73b420c19c",  # noqa: E501
    },
    {
        "file": "pytest-3.5.0.tar.gz",
        "hash": "sha256:fae491d1874f199537fd5872b5e1f0e74a009b979df9d53d1553fd03da1703e1",  # noqa: E501
    },
]
