# This test was originally written to
# check that a third party dependency
# gets priority over stdlib. But it was
# not clearly the right choice[1].
#
# 2026-01-09: We stuck with it for now,
# but it seems likely that we'll want
# to regress this task in favor of
# another.
#
# [1]: https://github.com/astral-sh/ruff/pull/22460#discussion_r2676343225
fullma<CURSOR: regex.fullmatch>
