# Regression test for https://github.com/astral-sh/ty/issues/2317
# Parser error recovery for this fuzzed input should not cause a panic.
arr[::[x for *b in y for (b: _
