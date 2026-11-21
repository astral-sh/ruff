class Documented:
    def documented(self) -> None:
        """Documentation."""
    value: int
    async def async_documented(self) -> None:
        """Documentation."""
    other: int
    def with_ellipsis(self) -> None:
        """Documentation."""
        ...
    without_added_space: int
    def with_body_comment(self) -> None:
        """Documentation."""
        # Inside the body.
    after_body_comment: int
    @property
    def property(self) -> int:
        """Documentation."""
    @property.setter
    def property(self, value: int) -> None: ...
    @overload
    def conditional(self, value: int) -> int:
        """Documentation."""
    if condition:
        @overload
        def conditional(self, value: str) -> str: ...
    if condition:
        async def nested(self) -> None:
            """Documentation."""
    after_conditional: int
    def final(self) -> None:
        """Documentation."""
    # A trailing member comment.
