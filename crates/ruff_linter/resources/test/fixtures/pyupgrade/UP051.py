# simple case, replace _T in signature and body
class Generic[_T]:
    buf: list[_T]

    def append(self, t: _T):
        self.buf.append(t)


# simple case, replace _T in signature and body
def second[_T](var: tuple[_T]) -> _T:
    y: _T = var[1]
    return y


# one diagnostic for each variable, comments are preserved
def many_generics[
    _T,  # first generic
    _U,  # second generic
](args):
    return args
