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

# Should not get a trailing comma
from mock import (
    mock,
    a,
    b,
    c
)

# These should not change:
import os, io
