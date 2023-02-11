import os
from pathlib import Path

(Path("") / "").open()

_ = Path(os.getcwd())

_ = Path(
    os.\
        getcwd()
)

_ = Path(
    os.getcwdb(),
)

# should not be unwrapped
_ = Path(os.getcwd(), hello='world')
