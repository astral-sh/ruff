"""
Should emit:
B032 - on lines 9, 10, 12, 13, 16-19
"""

# Flag these
dct = {"a": 1}

dct["b"]: 2
dct.b: 2

dct["b"]: "test"
dct.b: "test"

test = "test"
dct["b"]: test
dct["b"]: test.lower()
dct.b: test
dct.b: test.lower()

# Do not flag below
typed_dct: dict[str, int] = {"a": 1}
typed_dct["b"] = 2
typed_dct.b = 2


class TestClass:
    def test_self(self):
        self.test: int