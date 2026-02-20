from typing import type_check_only


@type_check_only
class UniquePrefixApple: pass

class UniquePrefixAzurous: pass


@type_check_only
def unique_prefix_apple() -> None: pass

def unique_prefix_azurous() -> None: pass


class Class:
    @type_check_only
    def meth_apple(self) -> None: pass

    def meth_azurous(self) -> None: pass
