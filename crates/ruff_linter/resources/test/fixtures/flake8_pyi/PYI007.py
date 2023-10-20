import sys

if sys.platform == "platform_name_1": ...  # OK

if sys.platform != "platform_name_2": ...  # OK

if sys.platform in ["linux"]: ...  # OK

if sys.platform > 3: ...  # OK

if sys.platform == 10.12: ...  # OK
