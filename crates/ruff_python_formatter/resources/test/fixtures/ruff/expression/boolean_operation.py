if (
    self._proc
    # has the child process finished?
    and self._returncode
    # the child process has finished, but the
    # transport hasn't been notified yet?
    and self._proc.poll()
):
    pass

if (
    self._proc
    and self._returncode
    and self._proc.poll()
    and self._proc
    and self._returncode
    and self._proc.poll()
):
    ...

if (
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    and aaaaaaaaaaaaaaaaa
    and aaaaaaaaaaaaaaaaaaaaaa
    and aaaaaaaaaaaaaaaaaaaaaaaa
    and aaaaaaaaaaaaaaaaaaaaaaaaaa
    and aaaaaaaaaaaaaaaaaaaaaaaaaaaa
):
    ...


if (
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaas
    and aaaaaaaaaaaaaaaaa
):
    ...


if [2222, 333] and [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
]:
    ...

if [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
] and [2222, 333]:
    pass

# Break right only applies for boolean operations with a left and right side
if (
    aaaaaaaaaaaaaaaaaaaaaaaaaa
    and bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    and ccccccccccccccccc
    and [dddddddddddddd, eeeeeeeeee, fffffffffffffff]
):
    pass
