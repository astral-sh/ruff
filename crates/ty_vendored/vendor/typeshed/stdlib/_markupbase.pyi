class ParserBase:
    def reset(self) -> None: ...
    def getpos(self) -> tuple[int, int]: ...
    def unknown_decl(self, data: str) -> None: ...
    def parse_comment(self, i: int, report: bool = True) -> int: ...  # undocumented
    def parse_declaration(self, i: int) -> int: ...  # undocumented
    def parse_marked_section(self, i: int, report: bool = True) -> int: ...  # undocumented
    def updatepos(self, i: int, j: int) -> int: ...  # undocumented
    lineno: int  # undocumented
    offset: int  # undocumented
