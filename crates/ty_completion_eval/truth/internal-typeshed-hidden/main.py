# This is a case where a symbol from an internal module appears
# before the desired symbol from `typing`.
#
# We use a slightly different example than the one reported in
# the issue to capture the deficiency via ranking. That is, in
# astral-sh/ty#1274, the (current) top suggestion is the correct one.
#
# ref: https://github.com/astral-sh/ty/issues/1274#issuecomment-3345923575
NoneTy<CURSOR: types.NoneType>
