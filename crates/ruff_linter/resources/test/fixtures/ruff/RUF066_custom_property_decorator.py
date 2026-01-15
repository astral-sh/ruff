from my_library import custom_property


class Example:
    @custom_property
    def missing_return(self):  # ERROR: No return
        x = 1

    @custom_property
    def with_return(self):  # OK: Has return
        return 1

    @property
    def builtin_property(self):  # ERROR: No return (builtin @property still works)
        x = 1
