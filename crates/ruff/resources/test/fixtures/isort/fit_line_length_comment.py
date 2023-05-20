import a
# Don't take this comment into account when determining whether the next import can fit on one line.
from b import c
from d import e  # Do take this comment into account when determining whether the next import can fit on one line.
# The next import fits on one line.
from f import g  # 012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ
# The next import doesn't fit on one line.
from h import i  # 012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9012ß9💣2ℝ9
