"""String table implementation for memory-efficient string storage in profiling data."""
class StringTable:
    """A string table for interning strings and reducing memory usage."""
    def intern(self, string: object) -> int:
        """Intern a string and return its index.

Args:
    string: The string to intern

Returns:
    int: The index of the string in the table
"""
    def get_string(self, index: int) -> str:
        """Get a string by its index.

Args:
    index: The index of the string

Returns:
    str: The string at the given index, or empty string if invalid
"""
    def get_strings(self) -> list[str]:
        """Get the list of all strings in the table.

Returns:
    list: A copy of the strings list
"""
    def __len__(self) -> int:
        """Return the number of strings in the table."""
