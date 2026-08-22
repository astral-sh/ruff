# The next import fits on one line once the trailing pragma is excluded from the width
# (the `# keep this` prefix still counts); in preview it should not be wrapped.
from aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa import x  # keep this  # noqa: TID251
# The next import exceeds the line length even without the trailing pragma
# (code plus the `# keep this` prefix is 89 columns); it must always be wrapped.
from bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb import x  # keep this  # noqa: TID251
