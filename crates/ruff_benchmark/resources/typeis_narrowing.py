"""Regression test for https://github.com/astral-sh/ty/issues/3120"""

from typing import Any, Literal, assert_never
from typing_extensions import TypeIs

Kind = Literal[
    "alpha_one",
    "alpha_two",
    "alpha_three",
    "alpha_four",
    "bravo_one",
    "bravo_two",
    "bravo_three",
    "bravo_four",
    "bravo_five",
    "bravo_six",
    "bravo_seven",
    "bravo_eight",
    "bravo_nine",
    "bravo_ten",
    "bravo_eleven",
    "bravo_twelve",
    "charlie_one",
    "charlie_two",
    "charlie_three",
    "charlie_four",
    "charlie_five",
    "charlie_six",
    "charlie_seven",
    "charlie_eight",
    "charlie_nine",
    "charlie_ten",
    "charlie_eleven",
    "charlie_twelve",
    "delta_one",
    "delta_two",
    "delta_three",
    "delta_four",
    "delta_five",
    "delta_six",
    "delta_seven",
    "delta_eight",
    "delta_nine",
    "delta_ten",
    "delta_eleven",
    "delta_twelve",
    "delta_thirteen",
    "delta_fourteen",
    "echo_one",
    "echo_two",
    "echo_three",
    "echo_four",
    "echo_five",
    "echo_six",
    "echo_seven",
    "echo_eight",
    "echo_nine",
    "echo_ten",
    "echo_eleven",
    "echo_twelve",
    "echo_thirteen",
    "echo_fourteen",
    "foxtrot_one",
    "foxtrot_two",
    "foxtrot_three",
    "foxtrot_four",
    "foxtrot_five",
    "foxtrot_six",
    "foxtrot_seven",
    "foxtrot_eight",
    "foxtrot_nine",
    "foxtrot_ten",
    "foxtrot_eleven",
    "foxtrot_twelve",
    "foxtrot_thirteen",
    "foxtrot_fourteen",
    "foxtrot_fifteen",
    "foxtrot_sixteen",
    "golf_one",
    "golf_two",
    "golf_three",
    "golf_four",
    "golf_five",
    "golf_six",
    "golf_seven",
    "golf_eight",
    "hotel_one",
    "hotel_two",
    "hotel_three",
    "hotel_four",
    "hotel_five",
]

CHARLIE = Literal[
    "charlie_one",
    "charlie_two",
    "charlie_three",
    "charlie_four",
    "charlie_five",
    "charlie_six",
    "charlie_seven",
]
DELTA = Literal[
    "delta_one", "delta_two", "delta_three", "delta_four", "delta_five", "delta_six"
]
ECHO = Literal[
    "echo_one", "echo_two", "echo_three", "echo_four", "echo_five", "echo_six"
]
FOXTROT = Literal["foxtrot_one", "foxtrot_two"]
CHARLIE_WIDE = Literal[
    "charlie_one",
    "charlie_two",
    "charlie_three",
    "charlie_four",
    "charlie_five",
    "charlie_six",
    "charlie_seven",
    "charlie_eight",
    "charlie_nine",
    "charlie_ten",
    "charlie_eleven",
    "charlie_twelve",
]
ALPHA = Literal[
    "alpha_one",
    "alpha_two",
    "alpha_three",
    "alpha_four",
    "bravo_one",
    "bravo_two",
    "bravo_three",
    "bravo_four",
    "bravo_five",
    "bravo_six",
    "bravo_seven",
    "bravo_eight",
    "bravo_nine",
    "bravo_ten",
    "bravo_eleven",
    "bravo_twelve",
    "delta_seven",
    "delta_eight",
    "echo_seven",
    "echo_eight",
]
GOLF = Literal[
    "golf_one",
    "golf_two",
    "golf_three",
    "golf_four",
    "golf_five",
    "golf_six",
    "golf_seven",
    "golf_eight",
]


def is_charlie(t: Kind) -> TypeIs[CHARLIE]:
    return t.startswith("charlie")


def is_delta(t: Kind) -> TypeIs[DELTA]:
    return t.startswith("delta")


def is_echo(t: Kind) -> TypeIs[ECHO]:
    return t.startswith("echo")


def is_foxtrot(t: Kind) -> TypeIs[FOXTROT]:
    return t.startswith("foxtrot")


def is_charlie_wide(t: Kind) -> TypeIs[CHARLIE_WIDE]:
    return t.startswith("charlie")


def is_alpha(t: Kind) -> TypeIs[ALPHA]:
    return t.startswith("alpha") or t.startswith("bravo")


def is_golf(t: Kind) -> TypeIs[GOLF]:
    return t.startswith("golf")


Action = Literal[
    "act_one",
    "act_two",
    "act_three",
    "act_four",
    "act_five",
    "act_six",
    "act_seven",
    "act_eight",
    "act_nine",
    "act_ten",
    "act_eleven",
    "act_twelve",
    "act_thirteen",
    "act_fourteen",
    "act_fifteen",
    "act_sixteen",
    "act_seventeen",
    "act_eighteen",
    "act_nineteen",
    "act_twenty",
]


def process(kind: Kind, action: Action | None, params: dict[str, Any]) -> str:
    if is_golf(kind):
        raise ValueError
    if is_alpha(kind) and action not in ["act_two", "act_five"]:
        raise ValueError

    if action is None:
        if is_foxtrot(kind):
            return "foxtrot"
        if is_echo(kind):
            return "echo"
        if is_delta(kind):
            return "delta"
        if is_charlie(kind):
            return "charlie"
        if kind == "bravo_one":
            action = "act_one"
        elif kind == "bravo_two":
            action = "act_eight"
        elif kind == "bravo_three":
            action = "act_three"
        elif kind == "alpha_one":
            action = "act_six"
        else:
            action = "act_one"
    else:
        match action:
            case "act_three":
                if kind != "bravo_three":
                    raise ValueError
            case "act_one" | "act_two":
                if kind not in ("alpha_one", "alpha_two", "alpha_three"):
                    raise ValueError
            case "act_four":
                if kind not in ("alpha_one", "alpha_two", "alpha_three"):
                    raise ValueError
            case "act_five":
                if kind not in ("alpha_one", "alpha_two", "alpha_three"):
                    raise ValueError
            case "act_six":
                if kind not in ("alpha_one", "alpha_two", "alpha_three", "alpha_four"):
                    raise ValueError
            case "act_seven":
                if kind != "bravo_one":
                    raise ValueError
            case "act_eight":
                if kind != "bravo_two":
                    raise ValueError
            case "act_nine" | "act_ten":
                if not is_charlie(kind):
                    raise ValueError
            case "act_eleven" | "act_twelve":
                if not is_delta(kind):
                    raise ValueError
                if params.get("version") == "2.1":
                    if kind in ("delta_nine", "delta_ten"):
                        if action == "act_eleven":
                            action = "act_thirteen"
                        elif action == "act_twelve":
                            action = "act_fourteen"
                        else:
                            assert_never(action)
                    else:
                        raise ValueError
            case "act_thirteen" | "act_fourteen":
                if not is_delta(kind):
                    raise ValueError
            case "act_fifteen" | "act_sixteen":
                if not is_echo(kind):
                    raise ValueError
            case "act_seventeen":
                if not is_charlie(kind):
                    raise ValueError
            case "act_eighteen":
                if not is_delta(kind):
                    raise ValueError
            case "act_nineteen":
                if not is_echo(kind):
                    raise ValueError
            case "act_twenty":
                if not is_foxtrot(kind):
                    raise ValueError
            case _ as never:
                assert_never(never)
        if is_charlie_wide(kind):
            pass

    return kind
