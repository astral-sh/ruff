# Unions in calls

## Union of return types

```py
def _(flag: bool):
    if flag:
        def f() -> int:
            return 1
    else:
        def f() -> str:
            return "foo"
    reveal_type(f())  # revealed: int | str
```

## Calling with an unknown union

```py
from nonexistent import f  # error: [unresolved-import] "Cannot resolve imported module `nonexistent`"

def coinflip() -> bool:
    return True

if coinflip():
    def f() -> int:
        return 1

reveal_type(f())  # revealed: Unknown | int
```

## Non-callable elements in a union

Calling a union with a non-callable element should emit a diagnostic.

```py
def _(flag: bool):
    if flag:
        f = 1
    else:
        def f() -> int:
            return 1
    x = f()  # error: [call-non-callable] "Object of type `Literal[1]` is not callable"
    reveal_type(x)  # revealed: Unknown | int
```

## Multiple non-callable elements in a union

Calling a union with multiple non-callable elements should mention all of them in the diagnostic.

```py
def _(flag: bool, flag2: bool):
    if flag:
        f = 1
    elif flag2:
        f = "foo"
    else:
        def f() -> int:
            return 1
    # error: [call-non-callable] "Object of type `Literal[1]` is not callable"
    # error: [call-non-callable] "Object of type `Literal["foo"]` is not callable"
    # revealed: Unknown | int
    reveal_type(f())
```

## All non-callable union elements

Calling a union with no callable elements can emit a simpler diagnostic.

```py
def _(flag: bool):
    if flag:
        f = 1
    else:
        f = "foo"

    x = f()  # error: [call-non-callable] "Object of type `Literal[1, "foo"]` is not callable"
    reveal_type(x)  # revealed: Unknown
```

## Mismatching signatures

Calling a union where the arguments don't match the signature of all variants.

```py
def f1(a: int) -> int:
    return a

def f2(a: str) -> str:
    return a

def _(flag: bool):
    if flag:
        f = f1
    else:
        f = f2

    # error: [invalid-argument-type] "Argument to function `f2` is incorrect: Expected `str`, found `Literal[3]`"
    x = f(3)
    reveal_type(x)  # revealed: int | str
```

## Any non-callable variant

```py
def f1(a: int): ...
def _(flag: bool):
    if flag:
        f = f1
    else:
        f = "This is a string literal"

    # error: [call-non-callable] "Object of type `Literal["This is a string literal"]` is not callable"
    x = f(3)
    reveal_type(x)  # revealed: Unknown
```

## Union of binding errors

```py
def f1(): ...
def f2(): ...
def _(flag: bool):
    if flag:
        f = f1
    else:
        f = f2

    # error: [too-many-positional-arguments] "Too many positional arguments to function `f1`: expected 0, got 1"
    # error: [too-many-positional-arguments] "Too many positional arguments to function `f2`: expected 0, got 1"
    x = f(3)
    reveal_type(x)  # revealed: Unknown
```

## One not-callable, one wrong argument

```py
class C: ...

def f1(): ...
def _(flag: bool):
    if flag:
        f = f1
    else:
        f = C()

    # error: [too-many-positional-arguments] "Too many positional arguments to function `f1`: expected 0, got 1"
    # error: [call-non-callable] "Object of type `C` is not callable"
    x = f(3)
    reveal_type(x)  # revealed: Unknown
```

## Union including a special-cased function

```py
def _(flag: bool):
    if flag:
        f = str
    else:
        f = repr
    reveal_type(str("string"))  # revealed: Literal["string"]
    reveal_type(repr("string"))  # revealed: Literal["'string'"]
    reveal_type(f("string"))  # revealed: Literal["string", "'string'"]
```

## Unions with literals and negations

```py
from typing import Literal
from ty_extensions import Not, AlwaysFalsy, static_assert, is_subtype_of, is_assignable_to

static_assert(is_subtype_of(Literal["a", ""], Literal["a", ""] | Not[AlwaysFalsy]))
static_assert(is_subtype_of(Not[AlwaysFalsy], Literal["", "a"] | Not[AlwaysFalsy]))
static_assert(is_subtype_of(Literal["a", ""], Not[AlwaysFalsy] | Literal["a", ""]))
static_assert(is_subtype_of(Not[AlwaysFalsy], Not[AlwaysFalsy] | Literal["a", ""]))

static_assert(is_subtype_of(Literal["a", ""], Literal["a", ""] | Not[Literal[""]]))
static_assert(is_subtype_of(Not[Literal[""]], Literal["a", ""] | Not[Literal[""]]))
static_assert(is_subtype_of(Literal["a", ""], Not[Literal[""]] | Literal["a", ""]))
static_assert(is_subtype_of(Not[Literal[""]], Not[Literal[""]] | Literal["a", ""]))

def _(
    a: Literal["a", ""] | Not[AlwaysFalsy],
    b: Literal["a", ""] | Not[Literal[""]],
    c: Literal[""] | Not[Literal[""]],
    d: Not[Literal[""]] | Literal[""],
    e: Literal["a"] | Not[Literal["a"]],
    f: Literal[b"b"] | Not[Literal[b"b"]],
    g: Not[Literal[b"b"]] | Literal[b"b"],
    h: Literal[42] | Not[Literal[42]],
    i: Not[Literal[42]] | Literal[42],
):
    reveal_type(a)  # revealed: Literal[""] | ~AlwaysFalsy
    reveal_type(b)  # revealed: object
    reveal_type(c)  # revealed: object
    reveal_type(d)  # revealed: object
    reveal_type(e)  # revealed: object
    reveal_type(f)  # revealed: object
    reveal_type(g)  # revealed: object
    reveal_type(h)  # revealed: object
    reveal_type(i)  # revealed: object
```

## Cannot use an argument as both a value and a type form

```py
from ty_extensions import is_singleton

def _(flag: bool):
    if flag:
        f = repr
    else:
        f = is_singleton
    # error: [conflicting-argument-forms] "Argument is used as both a value and a type form in call"
    reveal_type(f(int))  # revealed: str | Literal[False]
```

## Size limit on unions of literals

Beyond a certain size, large unions of literal types collapse to their nearest super-type (`int`,
`bytes`, `str`).

```py
from typing import Literal

def _(literals_2: Literal[0, 1], b: bool, flag: bool):
    literals_4 = 2 * literals_2 + literals_2  # Literal[0, 1, 2, 3]
    literals_16 = 4 * literals_4 + literals_4  # Literal[0, 1, .., 15]
    literals_64 = 4 * literals_16 + literals_4  # Literal[0, 1, .., 63]
    literals_128 = 2 * literals_64 + literals_2  # Literal[0, 1, .., 127]
    literals_256 = 2 * literals_128 + literals_2  # Literal[0, 1, .., 255]

    # Going beyond the MAX_NON_RECURSIVE_UNION_LITERALS limit (currently 256):
    reveal_type(literals_256 if flag else 256)  # revealed: int

    # Going beyond the limit when another type is already part of the union
    bool_and_literals_128 = b if flag else literals_128  # bool | Literal[0, 1, ..., 127]
    literals_128_shifted = literals_128 + 128  # Literal[128, 129, ..., 255]
    literals_256_shifted = literals_256 + 256  # Literal[256, 257, ..., 511]

    # Now union the two:
    two = bool_and_literals_128 if flag else literals_128_shifted
    # revealed: bool | Literal[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255]
    reveal_type(two)
    reveal_type(two if flag else literals_256_shifted)  # revealed: int
```

Recursively defined literal union types are widened earlier than non-recursively defined types for
faster convergence.

```py
class RecursiveAttr:
    def __init__(self):
        self.i = 0

    def update(self):
        self.i = self.i + 1

reveal_type(RecursiveAttr().i)  # revealed: Unknown | int

# Here are some recursive but saturating examples. Because it's difficult to statically determine whether literal unions saturate or diverge,
# we widen them early, even though they may actually be convergent.
class RecursiveAttr2:
    def __init__(self):
        self.i = 0

    def update(self):
        self.i = (self.i + 1) % 9

reveal_type(RecursiveAttr2().i)  # revealed: Unknown | Literal[0, 1, 2, 3, 4, 5, 6, 7, 8]

class RecursiveAttr3:
    def __init__(self):
        self.i = 0

    def update(self):
        self.i = (self.i + 1) % 10

# Going beyond the MAX_RECURSIVE_UNION_LITERALS limit:
reveal_type(RecursiveAttr3().i)  # revealed: Unknown | int
```

We set a much higher limit for non-recursive unions of enum literals, because huge enums are common
in generated code and it becomes frustrating if reachability analysis fails when matching over these
enums:

```py
from enum import Enum
from ty_extensions import Intersection, Not

class Huge(Enum):
    OPTION0 = "0"
    OPTION1 = "1"
    OPTION2 = "2"
    OPTION3 = "3"
    OPTION4 = "4"
    OPTION5 = "5"
    OPTION6 = "6"
    OPTION7 = "7"
    OPTION8 = "8"
    OPTION9 = "9"
    OPTION10 = "10"
    OPTION11 = "11"
    OPTION12 = "12"
    OPTION13 = "13"
    OPTION14 = "14"
    OPTION15 = "15"
    OPTION16 = "16"
    OPTION17 = "17"
    OPTION18 = "18"
    OPTION19 = "19"
    OPTION20 = "20"
    OPTION21 = "21"
    OPTION22 = "22"
    OPTION23 = "23"
    OPTION24 = "24"
    OPTION25 = "25"
    OPTION26 = "26"
    OPTION27 = "27"
    OPTION28 = "28"
    OPTION29 = "29"
    OPTION30 = "30"
    OPTION31 = "31"
    OPTION32 = "32"
    OPTION33 = "33"
    OPTION34 = "34"
    OPTION35 = "35"
    OPTION36 = "36"
    OPTION37 = "37"
    OPTION38 = "38"
    OPTION39 = "39"
    OPTION40 = "40"
    OPTION41 = "41"
    OPTION42 = "42"
    OPTION43 = "43"
    OPTION44 = "44"
    OPTION45 = "45"
    OPTION46 = "46"
    OPTION47 = "47"
    OPTION48 = "48"
    OPTION49 = "49"
    OPTION50 = "50"
    OPTION51 = "51"
    OPTION52 = "52"
    OPTION53 = "53"
    OPTION54 = "54"
    OPTION55 = "55"
    OPTION56 = "56"
    OPTION57 = "57"
    OPTION58 = "58"
    OPTION59 = "59"
    OPTION60 = "60"
    OPTION61 = "61"
    OPTION62 = "62"
    OPTION63 = "63"
    OPTION64 = "64"
    OPTION65 = "65"
    OPTION66 = "66"
    OPTION67 = "67"
    OPTION68 = "68"
    OPTION69 = "69"
    OPTION70 = "70"
    OPTION71 = "71"
    OPTION72 = "72"
    OPTION73 = "73"
    OPTION74 = "74"
    OPTION75 = "75"
    OPTION76 = "76"
    OPTION77 = "77"
    OPTION78 = "78"
    OPTION79 = "79"
    OPTION80 = "80"
    OPTION81 = "81"
    OPTION82 = "82"
    OPTION83 = "83"
    OPTION84 = "84"
    OPTION85 = "85"
    OPTION86 = "86"
    OPTION87 = "87"
    OPTION88 = "88"
    OPTION89 = "89"
    OPTION90 = "90"
    OPTION91 = "91"
    OPTION92 = "92"
    OPTION93 = "93"
    OPTION94 = "94"
    OPTION95 = "95"
    OPTION96 = "96"
    OPTION97 = "97"
    OPTION98 = "98"
    OPTION99 = "99"
    OPTION100 = "100"
    OPTION101 = "101"
    OPTION102 = "102"
    OPTION103 = "103"
    OPTION104 = "104"
    OPTION105 = "105"
    OPTION106 = "106"
    OPTION107 = "107"
    OPTION108 = "108"
    OPTION109 = "109"
    OPTION110 = "110"
    OPTION111 = "111"
    OPTION112 = "112"
    OPTION113 = "113"
    OPTION114 = "114"
    OPTION115 = "115"
    OPTION116 = "116"
    OPTION117 = "117"
    OPTION118 = "118"
    OPTION119 = "119"
    OPTION120 = "120"
    OPTION121 = "121"
    OPTION122 = "122"
    OPTION123 = "123"
    OPTION124 = "124"
    OPTION125 = "125"
    OPTION126 = "126"
    OPTION127 = "127"
    OPTION128 = "128"
    OPTION129 = "129"
    OPTION130 = "130"
    OPTION131 = "131"
    OPTION132 = "132"
    OPTION133 = "133"
    OPTION134 = "134"
    OPTION135 = "135"
    OPTION136 = "136"
    OPTION137 = "137"
    OPTION138 = "138"
    OPTION139 = "139"
    OPTION140 = "140"
    OPTION141 = "141"
    OPTION142 = "142"
    OPTION143 = "143"
    OPTION144 = "144"
    OPTION145 = "145"
    OPTION146 = "146"
    OPTION147 = "147"
    OPTION148 = "148"
    OPTION149 = "149"
    OPTION150 = "150"
    OPTION151 = "151"
    OPTION152 = "152"
    OPTION153 = "153"
    OPTION154 = "154"
    OPTION155 = "155"
    OPTION156 = "156"
    OPTION157 = "157"
    OPTION158 = "158"
    OPTION159 = "159"
    OPTION160 = "160"
    OPTION161 = "161"
    OPTION162 = "162"
    OPTION163 = "163"
    OPTION164 = "164"
    OPTION165 = "165"
    OPTION166 = "166"
    OPTION167 = "167"
    OPTION168 = "168"
    OPTION169 = "169"
    OPTION170 = "170"
    OPTION171 = "171"
    OPTION172 = "172"
    OPTION173 = "173"
    OPTION174 = "174"
    OPTION175 = "175"
    OPTION176 = "176"
    OPTION177 = "177"
    OPTION178 = "178"
    OPTION179 = "179"
    OPTION180 = "180"
    OPTION181 = "181"
    OPTION182 = "182"
    OPTION183 = "183"
    OPTION184 = "184"
    OPTION185 = "185"
    OPTION186 = "186"
    OPTION187 = "187"
    OPTION188 = "188"
    OPTION189 = "189"
    OPTION190 = "190"
    OPTION191 = "191"
    OPTION192 = "192"
    OPTION193 = "193"
    OPTION194 = "194"
    OPTION195 = "195"
    OPTION196 = "196"
    OPTION197 = "197"
    OPTION198 = "198"
    OPTION199 = "199"
    OPTION200 = "200"
    OPTION201 = "201"
    OPTION202 = "202"
    OPTION203 = "203"
    OPTION204 = "204"
    OPTION205 = "205"
    OPTION206 = "206"
    OPTION207 = "207"
    OPTION208 = "208"
    OPTION209 = "209"
    OPTION210 = "210"
    OPTION211 = "211"
    OPTION212 = "212"
    OPTION213 = "213"
    OPTION214 = "214"
    OPTION215 = "215"
    OPTION216 = "216"
    OPTION217 = "217"
    OPTION218 = "218"
    OPTION219 = "219"
    OPTION220 = "220"
    OPTION221 = "221"
    OPTION222 = "222"
    OPTION223 = "223"
    OPTION224 = "224"
    OPTION225 = "225"
    OPTION226 = "226"
    OPTION227 = "227"
    OPTION228 = "228"
    OPTION229 = "229"
    OPTION230 = "230"
    OPTION231 = "231"
    OPTION232 = "232"
    OPTION233 = "233"
    OPTION234 = "234"
    OPTION235 = "235"
    OPTION236 = "236"
    OPTION237 = "237"
    OPTION238 = "238"
    OPTION239 = "239"
    OPTION240 = "240"
    OPTION241 = "241"
    OPTION242 = "242"
    OPTION243 = "243"
    OPTION244 = "244"
    OPTION245 = "245"
    OPTION246 = "246"
    OPTION247 = "247"
    OPTION248 = "248"
    OPTION249 = "249"
    OPTION250 = "250"
    OPTION251 = "251"
    OPTION252 = "252"
    OPTION253 = "253"
    OPTION254 = "254"
    OPTION255 = "255"
    OPTION256 = "256"
    OPTION257 = "257"
    OPTION258 = "258"
    OPTION259 = "259"
    OPTION260 = "260"
    OPTION261 = "261"
    OPTION262 = "262"
    OPTION263 = "263"
    OPTION264 = "264"
    OPTION265 = "265"
    OPTION266 = "266"
    OPTION267 = "267"
    OPTION268 = "268"
    OPTION269 = "269"
    OPTION270 = "270"
    OPTION271 = "271"
    OPTION272 = "272"
    OPTION273 = "273"
    OPTION274 = "274"
    OPTION275 = "275"
    OPTION276 = "276"
    OPTION277 = "277"
    OPTION278 = "278"
    OPTION279 = "279"
    OPTION280 = "280"
    OPTION281 = "281"
    OPTION282 = "282"
    OPTION283 = "283"
    OPTION284 = "284"
    OPTION285 = "285"
    OPTION286 = "286"
    OPTION287 = "287"
    OPTION288 = "288"
    OPTION289 = "289"
    OPTION290 = "290"
    OPTION291 = "291"
    OPTION292 = "292"
    OPTION293 = "293"
    OPTION294 = "294"
    OPTION295 = "295"
    OPTION296 = "296"
    OPTION297 = "297"
    OPTION298 = "298"
    OPTION299 = "299"
    OPTION300 = "300"
    OPTION301 = "301"
    OPTION302 = "302"
    OPTION303 = "303"
    OPTION304 = "304"
    OPTION305 = "305"
    OPTION306 = "306"
    OPTION307 = "307"
    OPTION308 = "308"
    OPTION309 = "309"
    OPTION310 = "310"
    OPTION311 = "311"
    OPTION312 = "312"
    OPTION313 = "313"
    OPTION314 = "314"
    OPTION315 = "315"
    OPTION316 = "316"
    OPTION317 = "317"
    OPTION318 = "318"
    OPTION319 = "319"
    OPTION320 = "320"
    OPTION321 = "321"
    OPTION322 = "322"
    OPTION323 = "323"
    OPTION324 = "324"
    OPTION325 = "325"
    OPTION326 = "326"
    OPTION327 = "327"
    OPTION328 = "328"
    OPTION329 = "329"
    OPTION330 = "330"
    OPTION331 = "331"
    OPTION332 = "332"
    OPTION333 = "333"
    OPTION334 = "334"
    OPTION335 = "335"
    OPTION336 = "336"
    OPTION337 = "337"
    OPTION338 = "338"
    OPTION339 = "339"
    OPTION340 = "340"
    OPTION341 = "341"
    OPTION342 = "342"
    OPTION343 = "343"
    OPTION344 = "344"
    OPTION345 = "345"
    OPTION346 = "346"
    OPTION347 = "347"
    OPTION348 = "348"
    OPTION349 = "349"
    OPTION350 = "350"
    OPTION351 = "351"
    OPTION352 = "352"
    OPTION353 = "353"
    OPTION354 = "354"
    OPTION355 = "355"
    OPTION356 = "356"
    OPTION357 = "357"
    OPTION358 = "358"
    OPTION359 = "359"
    OPTION360 = "360"
    OPTION361 = "361"
    OPTION362 = "362"
    OPTION363 = "363"
    OPTION364 = "364"
    OPTION365 = "365"
    OPTION366 = "366"
    OPTION367 = "367"
    OPTION368 = "368"
    OPTION369 = "369"
    OPTION370 = "370"
    OPTION371 = "371"
    OPTION372 = "372"
    OPTION373 = "373"
    OPTION374 = "374"
    OPTION375 = "375"
    OPTION376 = "376"
    OPTION377 = "377"
    OPTION378 = "378"
    OPTION379 = "379"
    OPTION380 = "380"
    OPTION381 = "381"
    OPTION382 = "382"
    OPTION383 = "383"
    OPTION384 = "384"
    OPTION385 = "385"
    OPTION386 = "386"
    OPTION387 = "387"
    OPTION388 = "388"
    OPTION389 = "389"
    OPTION390 = "390"
    OPTION391 = "391"
    OPTION392 = "392"
    OPTION393 = "393"
    OPTION394 = "394"
    OPTION395 = "395"
    OPTION396 = "396"
    OPTION397 = "397"
    OPTION398 = "398"
    OPTION399 = "399"
    OPTION400 = "400"
    OPTION401 = "401"
    OPTION402 = "402"
    OPTION403 = "403"
    OPTION404 = "404"
    OPTION405 = "405"
    OPTION406 = "406"
    OPTION407 = "407"
    OPTION408 = "408"
    OPTION409 = "409"
    OPTION410 = "410"
    OPTION411 = "411"
    OPTION412 = "412"
    OPTION413 = "413"
    OPTION414 = "414"
    OPTION415 = "415"
    OPTION416 = "416"
    OPTION417 = "417"
    OPTION418 = "418"
    OPTION419 = "419"
    OPTION420 = "420"
    OPTION421 = "421"
    OPTION422 = "422"
    OPTION423 = "423"
    OPTION424 = "424"
    OPTION425 = "425"
    OPTION426 = "426"
    OPTION427 = "427"
    OPTION428 = "428"
    OPTION429 = "429"
    OPTION430 = "430"
    OPTION431 = "431"
    OPTION432 = "432"
    OPTION433 = "433"
    OPTION434 = "434"
    OPTION435 = "435"
    OPTION436 = "436"
    OPTION437 = "437"
    OPTION438 = "438"
    OPTION439 = "439"
    OPTION440 = "440"
    OPTION441 = "441"
    OPTION442 = "442"
    OPTION443 = "443"
    OPTION444 = "444"
    OPTION445 = "445"
    OPTION446 = "446"
    OPTION447 = "447"
    OPTION448 = "448"
    OPTION449 = "449"
    OPTION450 = "450"
    OPTION451 = "451"
    OPTION452 = "452"
    OPTION453 = "453"
    OPTION454 = "454"
    OPTION455 = "455"
    OPTION456 = "456"
    OPTION457 = "457"
    OPTION458 = "458"
    OPTION459 = "459"
    OPTION460 = "460"
    OPTION461 = "461"
    OPTION462 = "462"
    OPTION463 = "463"
    OPTION464 = "464"
    OPTION465 = "465"
    OPTION466 = "466"
    OPTION467 = "467"
    OPTION468 = "468"
    OPTION469 = "469"
    OPTION470 = "470"
    OPTION471 = "471"
    OPTION472 = "472"
    OPTION473 = "473"
    OPTION474 = "474"
    OPTION475 = "475"
    OPTION476 = "476"
    OPTION477 = "477"
    OPTION478 = "478"
    OPTION479 = "479"
    OPTION480 = "480"
    OPTION481 = "481"
    OPTION482 = "482"
    OPTION483 = "483"
    OPTION484 = "484"
    OPTION485 = "485"
    OPTION486 = "486"
    OPTION487 = "487"
    OPTION488 = "488"
    OPTION489 = "489"
    OPTION490 = "490"
    OPTION491 = "491"
    OPTION492 = "492"
    OPTION493 = "493"
    OPTION494 = "494"
    OPTION495 = "495"
    OPTION496 = "496"
    OPTION497 = "497"
    OPTION498 = "498"
    OPTION499 = "499"

def f(x: Intersection[Huge, Not[Literal[Huge.OPTION499]]]):
    # revealed: Literal[Huge.OPTION0, Huge.OPTION1, Huge.OPTION2, Huge.OPTION3, Huge.OPTION4, Huge.OPTION5, Huge.OPTION6, Huge.OPTION7, Huge.OPTION8, Huge.OPTION9, Huge.OPTION10, Huge.OPTION11, Huge.OPTION12, Huge.OPTION13, Huge.OPTION14, Huge.OPTION15, Huge.OPTION16, Huge.OPTION17, Huge.OPTION18, Huge.OPTION19, Huge.OPTION20, Huge.OPTION21, Huge.OPTION22, Huge.OPTION23, Huge.OPTION24, Huge.OPTION25, Huge.OPTION26, Huge.OPTION27, Huge.OPTION28, Huge.OPTION29, Huge.OPTION30, Huge.OPTION31, Huge.OPTION32, Huge.OPTION33, Huge.OPTION34, Huge.OPTION35, Huge.OPTION36, Huge.OPTION37, Huge.OPTION38, Huge.OPTION39, Huge.OPTION40, Huge.OPTION41, Huge.OPTION42, Huge.OPTION43, Huge.OPTION44, Huge.OPTION45, Huge.OPTION46, Huge.OPTION47, Huge.OPTION48, Huge.OPTION49, Huge.OPTION50, Huge.OPTION51, Huge.OPTION52, Huge.OPTION53, Huge.OPTION54, Huge.OPTION55, Huge.OPTION56, Huge.OPTION57, Huge.OPTION58, Huge.OPTION59, Huge.OPTION60, Huge.OPTION61, Huge.OPTION62, Huge.OPTION63, Huge.OPTION64, Huge.OPTION65, Huge.OPTION66, Huge.OPTION67, Huge.OPTION68, Huge.OPTION69, Huge.OPTION70, Huge.OPTION71, Huge.OPTION72, Huge.OPTION73, Huge.OPTION74, Huge.OPTION75, Huge.OPTION76, Huge.OPTION77, Huge.OPTION78, Huge.OPTION79, Huge.OPTION80, Huge.OPTION81, Huge.OPTION82, Huge.OPTION83, Huge.OPTION84, Huge.OPTION85, Huge.OPTION86, Huge.OPTION87, Huge.OPTION88, Huge.OPTION89, Huge.OPTION90, Huge.OPTION91, Huge.OPTION92, Huge.OPTION93, Huge.OPTION94, Huge.OPTION95, Huge.OPTION96, Huge.OPTION97, Huge.OPTION98, Huge.OPTION99, Huge.OPTION100, Huge.OPTION101, Huge.OPTION102, Huge.OPTION103, Huge.OPTION104, Huge.OPTION105, Huge.OPTION106, Huge.OPTION107, Huge.OPTION108, Huge.OPTION109, Huge.OPTION110, Huge.OPTION111, Huge.OPTION112, Huge.OPTION113, Huge.OPTION114, Huge.OPTION115, Huge.OPTION116, Huge.OPTION117, Huge.OPTION118, Huge.OPTION119, Huge.OPTION120, Huge.OPTION121, Huge.OPTION122, Huge.OPTION123, Huge.OPTION124, Huge.OPTION125, Huge.OPTION126, Huge.OPTION127, Huge.OPTION128, Huge.OPTION129, Huge.OPTION130, Huge.OPTION131, Huge.OPTION132, Huge.OPTION133, Huge.OPTION134, Huge.OPTION135, Huge.OPTION136, Huge.OPTION137, Huge.OPTION138, Huge.OPTION139, Huge.OPTION140, Huge.OPTION141, Huge.OPTION142, Huge.OPTION143, Huge.OPTION144, Huge.OPTION145, Huge.OPTION146, Huge.OPTION147, Huge.OPTION148, Huge.OPTION149, Huge.OPTION150, Huge.OPTION151, Huge.OPTION152, Huge.OPTION153, Huge.OPTION154, Huge.OPTION155, Huge.OPTION156, Huge.OPTION157, Huge.OPTION158, Huge.OPTION159, Huge.OPTION160, Huge.OPTION161, Huge.OPTION162, Huge.OPTION163, Huge.OPTION164, Huge.OPTION165, Huge.OPTION166, Huge.OPTION167, Huge.OPTION168, Huge.OPTION169, Huge.OPTION170, Huge.OPTION171, Huge.OPTION172, Huge.OPTION173, Huge.OPTION174, Huge.OPTION175, Huge.OPTION176, Huge.OPTION177, Huge.OPTION178, Huge.OPTION179, Huge.OPTION180, Huge.OPTION181, Huge.OPTION182, Huge.OPTION183, Huge.OPTION184, Huge.OPTION185, Huge.OPTION186, Huge.OPTION187, Huge.OPTION188, Huge.OPTION189, Huge.OPTION190, Huge.OPTION191, Huge.OPTION192, Huge.OPTION193, Huge.OPTION194, Huge.OPTION195, Huge.OPTION196, Huge.OPTION197, Huge.OPTION198, Huge.OPTION199, Huge.OPTION200, Huge.OPTION201, Huge.OPTION202, Huge.OPTION203, Huge.OPTION204, Huge.OPTION205, Huge.OPTION206, Huge.OPTION207, Huge.OPTION208, Huge.OPTION209, Huge.OPTION210, Huge.OPTION211, Huge.OPTION212, Huge.OPTION213, Huge.OPTION214, Huge.OPTION215, Huge.OPTION216, Huge.OPTION217, Huge.OPTION218, Huge.OPTION219, Huge.OPTION220, Huge.OPTION221, Huge.OPTION222, Huge.OPTION223, Huge.OPTION224, Huge.OPTION225, Huge.OPTION226, Huge.OPTION227, Huge.OPTION228, Huge.OPTION229, Huge.OPTION230, Huge.OPTION231, Huge.OPTION232, Huge.OPTION233, Huge.OPTION234, Huge.OPTION235, Huge.OPTION236, Huge.OPTION237, Huge.OPTION238, Huge.OPTION239, Huge.OPTION240, Huge.OPTION241, Huge.OPTION242, Huge.OPTION243, Huge.OPTION244, Huge.OPTION245, Huge.OPTION246, Huge.OPTION247, Huge.OPTION248, Huge.OPTION249, Huge.OPTION250, Huge.OPTION251, Huge.OPTION252, Huge.OPTION253, Huge.OPTION254, Huge.OPTION255, Huge.OPTION256, Huge.OPTION257, Huge.OPTION258, Huge.OPTION259, Huge.OPTION260, Huge.OPTION261, Huge.OPTION262, Huge.OPTION263, Huge.OPTION264, Huge.OPTION265, Huge.OPTION266, Huge.OPTION267, Huge.OPTION268, Huge.OPTION269, Huge.OPTION270, Huge.OPTION271, Huge.OPTION272, Huge.OPTION273, Huge.OPTION274, Huge.OPTION275, Huge.OPTION276, Huge.OPTION277, Huge.OPTION278, Huge.OPTION279, Huge.OPTION280, Huge.OPTION281, Huge.OPTION282, Huge.OPTION283, Huge.OPTION284, Huge.OPTION285, Huge.OPTION286, Huge.OPTION287, Huge.OPTION288, Huge.OPTION289, Huge.OPTION290, Huge.OPTION291, Huge.OPTION292, Huge.OPTION293, Huge.OPTION294, Huge.OPTION295, Huge.OPTION296, Huge.OPTION297, Huge.OPTION298, Huge.OPTION299, Huge.OPTION300, Huge.OPTION301, Huge.OPTION302, Huge.OPTION303, Huge.OPTION304, Huge.OPTION305, Huge.OPTION306, Huge.OPTION307, Huge.OPTION308, Huge.OPTION309, Huge.OPTION310, Huge.OPTION311, Huge.OPTION312, Huge.OPTION313, Huge.OPTION314, Huge.OPTION315, Huge.OPTION316, Huge.OPTION317, Huge.OPTION318, Huge.OPTION319, Huge.OPTION320, Huge.OPTION321, Huge.OPTION322, Huge.OPTION323, Huge.OPTION324, Huge.OPTION325, Huge.OPTION326, Huge.OPTION327, Huge.OPTION328, Huge.OPTION329, Huge.OPTION330, Huge.OPTION331, Huge.OPTION332, Huge.OPTION333, Huge.OPTION334, Huge.OPTION335, Huge.OPTION336, Huge.OPTION337, Huge.OPTION338, Huge.OPTION339, Huge.OPTION340, Huge.OPTION341, Huge.OPTION342, Huge.OPTION343, Huge.OPTION344, Huge.OPTION345, Huge.OPTION346, Huge.OPTION347, Huge.OPTION348, Huge.OPTION349, Huge.OPTION350, Huge.OPTION351, Huge.OPTION352, Huge.OPTION353, Huge.OPTION354, Huge.OPTION355, Huge.OPTION356, Huge.OPTION357, Huge.OPTION358, Huge.OPTION359, Huge.OPTION360, Huge.OPTION361, Huge.OPTION362, Huge.OPTION363, Huge.OPTION364, Huge.OPTION365, Huge.OPTION366, Huge.OPTION367, Huge.OPTION368, Huge.OPTION369, Huge.OPTION370, Huge.OPTION371, Huge.OPTION372, Huge.OPTION373, Huge.OPTION374, Huge.OPTION375, Huge.OPTION376, Huge.OPTION377, Huge.OPTION378, Huge.OPTION379, Huge.OPTION380, Huge.OPTION381, Huge.OPTION382, Huge.OPTION383, Huge.OPTION384, Huge.OPTION385, Huge.OPTION386, Huge.OPTION387, Huge.OPTION388, Huge.OPTION389, Huge.OPTION390, Huge.OPTION391, Huge.OPTION392, Huge.OPTION393, Huge.OPTION394, Huge.OPTION395, Huge.OPTION396, Huge.OPTION397, Huge.OPTION398, Huge.OPTION399, Huge.OPTION400, Huge.OPTION401, Huge.OPTION402, Huge.OPTION403, Huge.OPTION404, Huge.OPTION405, Huge.OPTION406, Huge.OPTION407, Huge.OPTION408, Huge.OPTION409, Huge.OPTION410, Huge.OPTION411, Huge.OPTION412, Huge.OPTION413, Huge.OPTION414, Huge.OPTION415, Huge.OPTION416, Huge.OPTION417, Huge.OPTION418, Huge.OPTION419, Huge.OPTION420, Huge.OPTION421, Huge.OPTION422, Huge.OPTION423, Huge.OPTION424, Huge.OPTION425, Huge.OPTION426, Huge.OPTION427, Huge.OPTION428, Huge.OPTION429, Huge.OPTION430, Huge.OPTION431, Huge.OPTION432, Huge.OPTION433, Huge.OPTION434, Huge.OPTION435, Huge.OPTION436, Huge.OPTION437, Huge.OPTION438, Huge.OPTION439, Huge.OPTION440, Huge.OPTION441, Huge.OPTION442, Huge.OPTION443, Huge.OPTION444, Huge.OPTION445, Huge.OPTION446, Huge.OPTION447, Huge.OPTION448, Huge.OPTION449, Huge.OPTION450, Huge.OPTION451, Huge.OPTION452, Huge.OPTION453, Huge.OPTION454, Huge.OPTION455, Huge.OPTION456, Huge.OPTION457, Huge.OPTION458, Huge.OPTION459, Huge.OPTION460, Huge.OPTION461, Huge.OPTION462, Huge.OPTION463, Huge.OPTION464, Huge.OPTION465, Huge.OPTION466, Huge.OPTION467, Huge.OPTION468, Huge.OPTION469, Huge.OPTION470, Huge.OPTION471, Huge.OPTION472, Huge.OPTION473, Huge.OPTION474, Huge.OPTION475, Huge.OPTION476, Huge.OPTION477, Huge.OPTION478, Huge.OPTION479, Huge.OPTION480, Huge.OPTION481, Huge.OPTION482, Huge.OPTION483, Huge.OPTION484, Huge.OPTION485, Huge.OPTION486, Huge.OPTION487, Huge.OPTION488, Huge.OPTION489, Huge.OPTION490, Huge.OPTION491, Huge.OPTION492, Huge.OPTION493, Huge.OPTION494, Huge.OPTION495, Huge.OPTION496, Huge.OPTION497, Huge.OPTION498]
    reveal_type(x)
```

## Simplifying gradually-equivalent types

If two types are gradually equivalent, we can keep just one of them in a union:

```py
from typing import Any, Union
from ty_extensions import Intersection, Not

def _(x: Union[Intersection[Any, Not[int]], Intersection[Any, Not[int]]]):
    reveal_type(x)  # revealed: Any & ~int
```

## Bidirectional Type Inference

```toml
[environment]
python-version = "3.12"
```

Type inference accounts for parameter type annotations across all signatures in a union.

```py
from typing import TypedDict, overload

class T(TypedDict):
    x: int

def _(flag: bool):
    if flag:
        def f(x: T) -> int:
            return 1
    else:
        def f(x: dict[str, int]) -> int:
            return 1
    x = f({"x": 1})
    reveal_type(x)  # revealed: int

    # error: [invalid-argument-type] "Argument to function `f` is incorrect: Expected `T`, found `dict[str, int] & dict[Unknown | str, Unknown | int]`"
    f({"y": 1})
```
