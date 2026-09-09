# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///

"""Add sidecar SHA-256 checksums to a cargo-dist local manifest.

The release workflow builds archives outside cargo-dist. Its global installer
generation needs their checksums in the manifest, so copy them from the sidecars
uploaded by the binary build jobs. Require a checksum for every downloaded
executable archive to avoid publishing installers with partial checksum coverage.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def read_sha256(path: Path) -> str:
    lines = path.read_text(encoding="utf-8").strip().splitlines()
    if len(lines) != 1:
        raise ValueError(f"expected one checksum in {path}")
    checksum = lines[0].split()[0]
    if re.fullmatch(r"[0-9a-fA-F]{64}", checksum) is None:
        raise ValueError(f"invalid SHA-256 checksum in {path}: {checksum!r}")
    return checksum.lower()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--artifacts-dir", type=Path, required=True)
    args = parser.parse_args()

    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    patched = 0
    for artifact_name, artifact in manifest["artifacts"].items():
        if artifact["kind"] != "executable-zip":
            continue
        # The manifest can include targets omitted by the custom binary jobs.
        if not (args.artifacts_dir / artifact_name).is_file():
            continue
        checksum = read_sha256(args.artifacts_dir / f"{artifact_name}.sha256")
        artifact.setdefault("checksums", {})["sha256"] = checksum
        patched += 1

    if patched == 0:
        raise ValueError(f"no executable archives found in {args.artifacts_dir}")

    args.manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"patched {patched} archive checksum(s) in {args.manifest}")


if __name__ == "__main__":
    main()
