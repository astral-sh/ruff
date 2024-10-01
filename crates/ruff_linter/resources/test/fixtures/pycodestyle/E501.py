"""Lorem ipsum dolor sit amet.

    https://github.com/PyCQA/pycodestyle/pull/258/files#diff-841c622497a8033d10152bfdfb15b20b92437ecdea21a260944ea86b77b51533

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.
"""

_ = """Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.

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

# OK
# A very long URL: https://loooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooong.url.com

# OK
# https://loooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooong.url.com

# OK
_ = """
Source: https://github.com/PyCQA/pycodestyle/pull/258/files#diff-841c622497a8033d10152bfdfb15b20b92437ecdea21a260944ea86b77b51533
"""

# OK
_ = """
[this-is-ok](https://github.com/PyCQA/pycodestyle/pull/258/files#diff-841c622497a8033d10152bfdfb15b20b92437ecdea21a260944ea86b77b51533)
[this is ok](https://github.com/PyCQA/pycodestyle/pull/258/files#diff-841c622497a8033d10152bfdfb15b20b92437ecdea21a260944ea86b77b51533)
"""


# OK
class Foo:
    """
    @see https://looooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooong.url.com

    :param dynamodb_scan_kwargs: kwargs pass to <https://boto3.amazonaws.com/v1/documentation/api/latest/reference/services/dynamodb.html#DynamoDB.Table.scan>
    """


# Error
class Bar:
    """
    This is a long sentence that ends with a shortened URL and, therefore, could easily be broken across multiple lines ([source](https://ruff.rs))
    """


# OK
# SPDX-FileCopyrightText: Copyright 2012-2015 Charlie Marsh <very-long-email-address@fake.com>
# SPDX-License-Identifier: a very long license identifier that exceeds the line length limit
