"""Test that shadowing an explicit re-export produces a warning."""

import foo as foo
import bar as foo
