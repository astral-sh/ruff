# The next import fits on one line once the pragma comment is excluded from the width;
# in preview it should not be wrapped (the `# noqa` must stay effective).
from aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa import x  # noqa: TID251
# The next import exceeds the line length even without the pragma comment;
# it must still be wrapped.
from bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb import x  # noqa: TID251


def f():
    # The next import fits on one line once the pragma comment is excluded from the
    # width, so in preview it should not be wrapped.
    from cccccccccccccccccccccccccccccc.ccccccccccccccccccccccccccccccccccccc import bar  # noqa: PLC0415
    bar()
