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

import mock, sys, os, io

from foo import (
    sys,
    os,
    io,
    math,
    random,
    datetime,
    calendar,
    functools,
    itertools,
    copy,
    graphlib,
    collections,
    mock
)


# These should not change:
import os, io
