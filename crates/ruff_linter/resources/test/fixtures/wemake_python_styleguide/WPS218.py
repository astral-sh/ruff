def function():  # has two asserts
    def factory():  # has one assert
        assert one()
    assert two()