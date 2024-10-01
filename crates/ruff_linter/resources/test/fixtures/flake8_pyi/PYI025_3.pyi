"""
Tests for PYI025 where the import is marked as re-exported
through usage of a "redundant" `import Set as Set` alias
"""

from collections.abc import Set as Set  # PYI025 triggered but fix is not marked as safe
