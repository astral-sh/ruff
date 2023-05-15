from typing import Dict, List, Optional, Set, Union, cast


def main() -> None:
    a_list: List[Optional[str]] = []
    a_list.append("hello")
    a_dict = cast(Dict[int | None, Union[int, Set[bool]]], {})
    a_dict[1] = {True, False}
