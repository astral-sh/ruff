class Outer:
    def outer_function(self):
        return Inner().inner_function()

    class Inner:
        def inner_function(self):
            return inner_dependency()

        def inner_dependency():
            return 100
