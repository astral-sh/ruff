# This is similar to the `numpy-array` test case,
# where the completions returned don't contain
# the expected symbol at all.
ZQZQZQ_<CURSOR: sub1.ZQZQZQ_SOMETHING_IMPORTANT>

import sub1
# This works though, so ty sees the symbol where
# as our auto-import symbol finder does not.
sub1.ZQZQZQ_<CURSOR: ZQZQZQ_SOMETHING_IMPORTANT>
