import sys
from re import error as error
from typing import Final
from typing_extensions import Self

MAXGROUPS: Final[int]

MAGIC: Final[int]

class _NamedIntConstant(int):
    name: str
    def __new__(cls, value: int, name: str) -> Self: ...

MAXREPEAT: Final[_NamedIntConstant]
OPCODES: list[_NamedIntConstant]
ATCODES: list[_NamedIntConstant]
CHCODES: list[_NamedIntConstant]
OP_IGNORE: dict[_NamedIntConstant, _NamedIntConstant]
OP_LOCALE_IGNORE: dict[_NamedIntConstant, _NamedIntConstant]
OP_UNICODE_IGNORE: dict[_NamedIntConstant, _NamedIntConstant]
AT_MULTILINE: dict[_NamedIntConstant, _NamedIntConstant]
AT_LOCALE: dict[_NamedIntConstant, _NamedIntConstant]
AT_UNICODE: dict[_NamedIntConstant, _NamedIntConstant]
CH_LOCALE: dict[_NamedIntConstant, _NamedIntConstant]
CH_UNICODE: dict[_NamedIntConstant, _NamedIntConstant]
if sys.version_info >= (3, 14):
    CH_NEGATE: dict[_NamedIntConstant, _NamedIntConstant]
# flags
if sys.version_info < (3, 13):
    SRE_FLAG_TEMPLATE: Final = 1
SRE_FLAG_IGNORECASE: Final = 2
SRE_FLAG_LOCALE: Final = 4
SRE_FLAG_MULTILINE: Final = 8
SRE_FLAG_DOTALL: Final = 16
SRE_FLAG_UNICODE: Final = 32
SRE_FLAG_VERBOSE: Final = 64
SRE_FLAG_DEBUG: Final = 128
SRE_FLAG_ASCII: Final = 256
# flags for INFO primitive
SRE_INFO_PREFIX: Final = 1
SRE_INFO_LITERAL: Final = 2
SRE_INFO_CHARSET: Final = 4

# Stubgen above; manually defined constants below (dynamic at runtime)

# from OPCODES
FAILURE: Final[_NamedIntConstant]
SUCCESS: Final[_NamedIntConstant]
ANY: Final[_NamedIntConstant]
ANY_ALL: Final[_NamedIntConstant]
ASSERT: Final[_NamedIntConstant]
ASSERT_NOT: Final[_NamedIntConstant]
AT: Final[_NamedIntConstant]
BRANCH: Final[_NamedIntConstant]
if sys.version_info < (3, 11):
    CALL: Final[_NamedIntConstant]
CATEGORY: Final[_NamedIntConstant]
CHARSET: Final[_NamedIntConstant]
BIGCHARSET: Final[_NamedIntConstant]
GROUPREF: Final[_NamedIntConstant]
GROUPREF_EXISTS: Final[_NamedIntConstant]
GROUPREF_IGNORE: Final[_NamedIntConstant]
IN: Final[_NamedIntConstant]
IN_IGNORE: Final[_NamedIntConstant]
INFO: Final[_NamedIntConstant]
JUMP: Final[_NamedIntConstant]
LITERAL: Final[_NamedIntConstant]
LITERAL_IGNORE: Final[_NamedIntConstant]
MARK: Final[_NamedIntConstant]
MAX_UNTIL: Final[_NamedIntConstant]
MIN_UNTIL: Final[_NamedIntConstant]
NOT_LITERAL: Final[_NamedIntConstant]
NOT_LITERAL_IGNORE: Final[_NamedIntConstant]
NEGATE: Final[_NamedIntConstant]
RANGE: Final[_NamedIntConstant]
REPEAT: Final[_NamedIntConstant]
REPEAT_ONE: Final[_NamedIntConstant]
SUBPATTERN: Final[_NamedIntConstant]
MIN_REPEAT_ONE: Final[_NamedIntConstant]
if sys.version_info >= (3, 11):
    ATOMIC_GROUP: Final[_NamedIntConstant]
    POSSESSIVE_REPEAT: Final[_NamedIntConstant]
    POSSESSIVE_REPEAT_ONE: Final[_NamedIntConstant]
RANGE_UNI_IGNORE: Final[_NamedIntConstant]
GROUPREF_LOC_IGNORE: Final[_NamedIntConstant]
GROUPREF_UNI_IGNORE: Final[_NamedIntConstant]
IN_LOC_IGNORE: Final[_NamedIntConstant]
IN_UNI_IGNORE: Final[_NamedIntConstant]
LITERAL_LOC_IGNORE: Final[_NamedIntConstant]
LITERAL_UNI_IGNORE: Final[_NamedIntConstant]
NOT_LITERAL_LOC_IGNORE: Final[_NamedIntConstant]
NOT_LITERAL_UNI_IGNORE: Final[_NamedIntConstant]
MIN_REPEAT: Final[_NamedIntConstant]
MAX_REPEAT: Final[_NamedIntConstant]

# from ATCODES
AT_BEGINNING: Final[_NamedIntConstant]
AT_BEGINNING_LINE: Final[_NamedIntConstant]
AT_BEGINNING_STRING: Final[_NamedIntConstant]
AT_BOUNDARY: Final[_NamedIntConstant]
AT_NON_BOUNDARY: Final[_NamedIntConstant]
AT_END: Final[_NamedIntConstant]
AT_END_LINE: Final[_NamedIntConstant]
AT_END_STRING: Final[_NamedIntConstant]
AT_LOC_BOUNDARY: Final[_NamedIntConstant]
AT_LOC_NON_BOUNDARY: Final[_NamedIntConstant]
AT_UNI_BOUNDARY: Final[_NamedIntConstant]
AT_UNI_NON_BOUNDARY: Final[_NamedIntConstant]

# from CHCODES
CATEGORY_DIGIT: Final[_NamedIntConstant]
CATEGORY_NOT_DIGIT: Final[_NamedIntConstant]
CATEGORY_SPACE: Final[_NamedIntConstant]
CATEGORY_NOT_SPACE: Final[_NamedIntConstant]
CATEGORY_WORD: Final[_NamedIntConstant]
CATEGORY_NOT_WORD: Final[_NamedIntConstant]
CATEGORY_LINEBREAK: Final[_NamedIntConstant]
CATEGORY_NOT_LINEBREAK: Final[_NamedIntConstant]
CATEGORY_LOC_WORD: Final[_NamedIntConstant]
CATEGORY_LOC_NOT_WORD: Final[_NamedIntConstant]
CATEGORY_UNI_DIGIT: Final[_NamedIntConstant]
CATEGORY_UNI_NOT_DIGIT: Final[_NamedIntConstant]
CATEGORY_UNI_SPACE: Final[_NamedIntConstant]
CATEGORY_UNI_NOT_SPACE: Final[_NamedIntConstant]
CATEGORY_UNI_WORD: Final[_NamedIntConstant]
CATEGORY_UNI_NOT_WORD: Final[_NamedIntConstant]
CATEGORY_UNI_LINEBREAK: Final[_NamedIntConstant]
CATEGORY_UNI_NOT_LINEBREAK: Final[_NamedIntConstant]
