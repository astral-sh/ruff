# Issue: https://github.com/astral-sh/ruff/issues/16247#event-16362806498

import os
import site
import sys
import sysconfig

site.addsitedir(
    os.path.join(
        os.path.dirname(os.path.dirname(__file__)),
        sysconfig.get_path("purelib", vars={"base": "."}),
    )
)

from mypkg.__main__ import main

if __name__ == "__main__":
    sys.argv[0] = sys.argv[0].removesuffix(".py")
    sys.exit(main())