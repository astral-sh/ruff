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
