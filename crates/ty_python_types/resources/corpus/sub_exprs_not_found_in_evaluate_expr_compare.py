# This is a regression test for `infer_expression_types`.
# ref: https://github.com/astral-sh/ruff/pull/18041#discussion_r2094573989

class C:
    def f(self, other: "C"):
        if self.a > other.b or self.b:
            return False
        if self:
            return True

C().a
