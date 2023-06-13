from abc import ABCMeta

import pydantic


class Class:
    def badAllowed(this):
        pass

    def stillBad(this):
        pass

    if False:

        def badAllowed(this):
            pass

        def stillBad(this):
            pass

    @pydantic.validator
    def badAllowed(cls, my_field: str) -> str:
        pass

    @pydantic.validator
    def stillBad(cls, my_field: str) -> str:
        pass

    @pydantic.validator("my_field")
    def badAllowed(cls, my_field: str) -> str:
        pass

    @pydantic.validator("my_field")
    def stillBad(cls, my_field: str) -> str:
        pass

class PosOnlyClass:
    def badAllowed(this, blah, /, self, something: str):
        pass

    def stillBad(this, blah, /, self, something: str):
        pass
