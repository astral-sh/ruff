# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Regenerate the offline wheel fixtures for the `uv check` integration tests."""

from __future__ import annotations

from pathlib import Path
from zipfile import ZIP_STORED, ZipFile, ZipInfo


def main() -> None:
    for distribution, module, dependencies in [
        ("indirect-dependency", "indirect_module", []),
        ("direct-dependency", "direct_module", ["indirect-dependency"]),
    ]:
        prefix = f"{distribution.replace('-', '_')}-0.1.0"
        metadata = f"Metadata-Version: 2.1\nName: {distribution}\nVersion: 0.1.0\n"
        metadata += "".join(f"Requires-Dist: {name}\n" for name in dependencies)
        files = {
            f"{module}.py": "value: int = 1\n",
            f"{prefix}.dist-info/METADATA": metadata,
            f"{prefix}.dist-info/WHEEL": (
                "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n"
            ),
        }
        record_path = f"{prefix}.dist-info/RECORD"
        files[record_path] = "".join(f"{path},,\n" for path in [*files, record_path])

        destination = Path(__file__).parent / f"{prefix}-py3-none-any.whl"
        with ZipFile(destination, "w", compression=ZIP_STORED) as wheel:
            for path, contents in files.items():
                entry = ZipInfo(path, date_time=(1980, 1, 1, 0, 0, 0))
                entry.create_system = 3
                entry.external_attr = 0o100644 << 16
                wheel.writestr(entry, contents)


if __name__ == "__main__":
    main()
