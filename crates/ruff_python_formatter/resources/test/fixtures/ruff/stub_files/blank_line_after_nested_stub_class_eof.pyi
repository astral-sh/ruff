# A separate file to test out the behavior when there are a mix of blank lines
# and comments at EOF just after a nested stub class.

class Top:
    class Nested1:
        class Nested12:
            pass
        # comment
    class Nested2:
        pass



# comment



