#!/usr/bin/env -S uv run
print("hello world")

#!/usr/bin/env uv --offline run
print("offline")

#!/usr/bin/env uv --color=auto run
print("color")

#!/usr/bin/env uv --quiet run --script
print("quiet run script")

#!/usr/bin/env uv tool run
print("uv tool")

#!/usr/bin/env uvx
print("uvx")

#!/usr/bin/env uvx --quiet
print("uvx quiet")

#!/usr/bin/env uv_not_really_run
print("this should fail")
