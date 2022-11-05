"""Lorem ipsum dolor sit amet.

    https://github.com/PyCQA/pycodestyle/pull/258/files#diff-841c622497a8033d10152bfdfb15b20b92437ecdea21a260944ea86b77b51533

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
"""

_ = """Lorem ipsum dolor sit amet.

    https://github.com/PyCQA/pycodestyle/pull/258/files#diff-841c622497a8033d10152bfdfb15b20b92437ecdea21a260944ea86b77b51533

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
"""  # noqa: E501

_ = "---------------------------------------------------------------------------AAAAAAA"
_ = "---------------------------------------------------------------------------亜亜亜亜亜亜亜"


def caller(string: str) -> None:
    assert string is not None


caller(
    """
    Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
    """,
)


caller(
    """
    Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
    """,  # noqa: E501
)

multiple_strings_per_line = {
    # OK
    "Lorem ipsum dolor": "sit amet",
    # E501 Line too long
    "Lorem ipsum dolor": "sit amet consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
    # E501 Line too long
    "Lorem ipsum dolor": """
sit amet  consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
""",
    # OK
    "Lorem ipsum dolor": "sit amet consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",  # noqa: E501
    # OK
    "Lorem ipsum dolor": """
sit amet  consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
""",  # noqa: E501
}
