# OK

import pytest
import pytest as pytest
from notpytest import fixture
from . import fixture
from .pytest import fixture

# Error

import pytest as other_name
from pytest import fixture
from pytest import fixture as other_name
