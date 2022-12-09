def f() -> None:
    # Valid
    a = 1  # noqa

    # Valid
    b = 2  # noqa: F841

    # Invalid
    c = 1  # noqa
    print(c)

    # Invalid
    d = 1  # noqa: E501

    # Invalid
    d = 1  # noqa: F841, E501

    # Invalid (and unimplemented)
    d = 1  # noqa: F841, W191

    # Invalid (but external)
    d = 1  # noqa: F841, V101

    # fmt: off
    # Invalid - no space before #
    d = 1# noqa: E501

    # Invalid - many spaces before #
    d = 1                       # noqa: E501
    # fmt: on


# Valid
_ = """Lorem ipsum dolor sit amet.

    https://github.com/PyCQA/pycodestyle/pull/258/files#diff-841c622497a8033d10152bfdfb15b20b92437ecdea21a260944ea86b77b51533

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
"""  # noqa: E501

# Valid
_ = """Lorem ipsum dolor sit amet.

    https://github.com/PyCQA/pycodestyle/pull/258/files#diff-841c622497a8033d10152bfdfb15b20b92437ecdea21a260944ea86b77b51533

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
"""  # noqa

# Invalid
_ = """Lorem ipsum dolor sit amet.

    https://github.com/PyCQA/pycodestyle/pull/258/files#diff-841c622497a8033d10152bfdfb15b20b92437ecdea21a260944ea86b77b51533

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
"""  # noqa: E501, F841

# Invalid
_ = """Lorem ipsum dolor sit amet.

    https://github.com/PyCQA/pycodestyle/pull/258/files#diff-841c622497a8033d10152bfdfb15b20b92437ecdea21a260944ea86b77b51533

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.
"""  # noqa: E501

# Invalid
_ = """Lorem ipsum dolor sit amet.

    https://github.com/PyCQA/pycodestyle/pull/258/files#diff-841c622497a8033d10152bfdfb15b20b92437ecdea21a260944ea86b77b51533

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.
"""  # noqa
