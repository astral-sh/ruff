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

# Valid
# this is a veryyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy long comment  # noqa: E501

# Valid
_ = """Here's a source: https://github.com/ethereum/web3.py/blob/ffe59daf10edc19ee5f05227b25bac8d090e8aa4/web3/_utils/events.py#L201

May raise:
- DeserializationError if the abi string is invalid or abi or log topics/data do not match
"""  # noqa: E501

import collections  # noqa
import os  # noqa: F401, RUF100
import shelve  # noqa: RUF100
import sys  # noqa: F401, RUF100

print(sys.path)
