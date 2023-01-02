# These should be changed
if True:
    import mock

if True:
    import mock, sys

# This goes to from unitest import mock
import mock.mock

# Mock should go on a new line as `from unittest import mock`
import contextlib, mock, sys

# Mock should go on a new line as `from unittest import mock`
import mock, sys
x = "This code should be preserved one line below the mock"

# Mock should go on a new line as `from unittest import mock`
from mock import mock

# Should keep trailing comma
from mock import (
    mock,
    a,
    b,
    c,
)
from mock import (
    a,
    b,
    c,
    mock,
)

# Should not get a trailing comma
from mock import (
    mock,
    a,
    b,
    c
)
from mock import (
    a,
    b,
    c,
    mock
)
from mock import mock, a, b, c
from mock import a, b, c, mock

if True:
    if False:
        from mock import (
            mock,
            a,
            b,
            c
        )

# These should not change:
import os, io

# Mock should go on a new line as `from unittest import mock`
import mock, mock

# Mock should go on a new line as `from unittest import mock as foo`
import mock as foo

# Mock should go on a new line as `from unittest import mock as foo`
from mock import mock as foo

if True:
    # This should yield multiple, aliased imports.
    import mock as foo, mock as bar, mock

    # This should yield multiple, aliased imports, and preserve `os`.
    import mock as foo, mock as bar, mock, os

if True:
    # This should yield multiple, aliased imports.
    from mock import mock as foo, mock as bar, mock


# This should be unchanged.
x = mock.Mock()

# This should change to `mock.Mock`().
x = mock.mock.Mock()
