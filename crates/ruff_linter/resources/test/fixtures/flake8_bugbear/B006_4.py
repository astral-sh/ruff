# formfeed indent
# https://github.com/astral-sh/ruff/issues/7455#issuecomment-1722458825
# This is technically a stylist bug (and has a test there), but it surfaced in B006


class FormFeedIndent:
   def __init__(self, a=[]):
        print(a)

