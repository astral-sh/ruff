import sys

if sys.platform == "platform_name_1": ...  # OK

if sys.platform != "platform_name_2": ...  # OK

if sys.platform in ["linux"]: ...  # Error: PYI007 Unrecognized sys.platform check

if sys.platform > 3: ...  # Error: PYI007 Unrecognized sys.platform check

if sys.platform == 10.12: ...  # Error: PYI007 Unrecognized sys.platform check
