#!/usr/bin/env python3
"""Apply BOLT optimization to Ruff's Linux executable and its existing wheel."""

from __future__ import annotations

import argparse
import base64
import copy
import csv
import hashlib
import io
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

from build_ruff_pgo import REPOSITORY_ROOT, run, rustc_host

ELF_MAGIC = b"\x7fELF"
IN_PLACE_MESSAGE = "BOLT-INFO: using original .text for new code"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--target-dir", type=Path, required=True)
    parser.add_argument("--wheels-dir", type=Path, required=True)
    parser.add_argument("--binary", type=Path)
    args = parser.parse_args()

    target_dir = args.target_dir.resolve()
    wheels_dir = args.wheels_dir.resolve()
    binary = (
        args.binary or REPOSITORY_ROOT / "target" / args.target / "release" / "ruff"
    ).resolve()
    corpus_arguments = target_dir / "ruff-pgo.args"
    pgo_profile = target_dir / "ruff.profdata"
    for required in (binary, corpus_arguments, pgo_profile):
        if not required.is_file():
            raise RuntimeError(f"Missing BOLT prerequisite: {required}")

    host = rustc_host()
    if args.target != host:
        parser.error(f"BOLT training requires host target {host}, got {args.target}")

    wheels = sorted(wheels_dir.glob("ruff-*.whl"))
    if not wheels:
        raise RuntimeError(f"No Ruff wheels found in {wheels_dir}")

    bolt = find_executable("llvm-bolt")
    merge_fdata = bolt.parent / "merge-fdata"
    if not executable(merge_fdata):
        merge_fdata = find_executable("merge-fdata")
    runtime = bolt.parent.parent / "lib" / "libbolt_rt_instr.a"
    if not runtime.is_file():
        raise RuntimeError(f"BOLT instrumentation runtime not found: {runtime}")

    sysroot = subprocess.run(
        ["rustc", "--print", "sysroot"],
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    llvm_strip = Path(sysroot) / "lib" / "rustlib" / host / "bin" / "llvm-strip"
    if not executable(llvm_strip):
        raise RuntimeError(
            f"Rust LLVM strip not found: {llvm_strip}; "
            "run `rustup component add llvm-tools-preview`"
        )

    readelf = find_executable("readelf")
    validate_input(readelf, binary)
    original_version = version(binary)
    environment = os.environ.copy()
    bolt_dir = target_dir / "bolt"
    bolt_dir.mkdir(parents=True, exist_ok=True)
    instrumented = bolt_dir / "ruff.instrumented"
    profile_prefix = bolt_dir / "ruff.fdata"
    merged_profile = bolt_dir / "ruff.merged.fdata"
    optimized = bolt_dir / "ruff.bolt.unstripped"
    for profile in bolt_dir.glob("ruff.fdata*"):
        if profile.is_file():
            profile.unlink()

    print("Instrumenting Ruff with BOLT", flush=True)
    run(
        [
            str(bolt),
            str(binary),
            "-instrument",
            f"--instrumentation-file={profile_prefix}",
            "--instrumentation-file-append-pid",
            f"--runtime-instrumentation-lib={runtime}",
            "-o",
            str(instrumented),
        ],
        environment=environment,
    )

    common_arguments = [
        "--isolated",
        "--target-version",
        "py314",
        "--no-cache",
        "--silent",
    ]
    run(
        [
            str(instrumented),
            "check",
            *common_arguments,
            "--exit-zero",
            f"@{corpus_arguments}",
        ],
        environment=environment,
    )
    run(
        [
            str(instrumented),
            "format",
            *common_arguments,
            "--check",
            f"@{corpus_arguments}",
        ],
        environment=environment,
        allowed_exit_codes=(0, 1),
    )

    profiles = sorted(bolt_dir.glob("ruff.fdata*"))
    if len(profiles) < 2 or any(profile.stat().st_size == 0 for profile in profiles):
        raise RuntimeError("Expected complete BOLT profiles from check and format")
    print(f"Merging {len(profiles)} BOLT profiles", flush=True)
    with merged_profile.open("wb") as output:
        subprocess.run(
            [str(merge_fdata), *map(str, profiles)],
            cwd=REPOSITORY_ROOT,
            env=environment,
            stdout=output,
            check=True,
        )
    if merged_profile.stat().st_size == 0:
        raise RuntimeError("BOLT profile merger produced an empty profile")

    command = [
        str(bolt),
        str(binary),
        f"-data={merged_profile}",
        "-o",
        str(optimized),
        "-reorder-blocks=ext-tsp",
        "-reorder-functions=cdsort",
        "-split-functions",
        "-split-strategy=cdsplit",
        "-split-all-cold",
        "-jump-tables=move",
        "-icf=all",
        "-dyno-stats",
        "--use-old-text",
        "--no-huge-pages",
    ]
    print("Optimizing Ruff with BOLT", flush=True)
    result = subprocess.run(
        command,
        cwd=REPOSITORY_ROOT,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=False,
    )
    print(result.stdout, end="", flush=True)
    if result.returncode:
        raise subprocess.CalledProcessError(result.returncode, command)
    if IN_PLACE_MESSAGE not in result.stdout:
        raise RuntimeError("BOLT could not reuse the original executable code")

    temporary_binary = temporary_path(binary.parent, ".ruff-bolt-", ".tmp")
    staged_wheels: list[tuple[Path, Path]] = []
    try:
        run(
            [
                str(llvm_strip),
                "--strip-all",
                "-o",
                str(temporary_binary),
                str(optimized),
            ],
            environment=environment,
        )
        temporary_binary.chmod(binary.stat().st_mode & 0o777)
        validate_security(readelf, temporary_binary)
        if version(temporary_binary) != original_version:
            raise RuntimeError("BOLT changed Ruff's executable version")

        smoke_input = bolt_dir / "smoke.py"
        smoke_input.write_text("value = 1\n", encoding="utf-8")
        run(
            [
                str(temporary_binary),
                "check",
                "--isolated",
                "--no-cache",
                "--silent",
                str(smoke_input),
            ],
            environment=environment,
        )
        run(
            [
                str(temporary_binary),
                "format",
                "--isolated",
                "--no-cache",
                "--check",
                "--silent",
                str(smoke_input),
            ],
            environment=environment,
        )

        for wheel in wheels:
            staged_wheels.append((wheel, rebuild_wheel(wheel, temporary_binary)))

        for wheel, staged in staged_wheels:
            staged.replace(wheel)
        temporary_binary.replace(binary)
        print(f"BOLT-optimized Ruff: {binary} ({binary.stat().st_size} bytes)")
    finally:
        temporary_binary.unlink(missing_ok=True)
        for _, staged in staged_wheels:
            staged.unlink(missing_ok=True)


def executable(path: Path) -> bool:
    return path.is_file() and os.access(path, os.X_OK)


def find_executable(name: str) -> Path:
    for candidate in (name, f"{name}-22"):
        if resolved := shutil.which(candidate):
            return Path(resolved).resolve()
    raise RuntimeError(f"Required executable not found on PATH: {name}")


def validate_input(readelf: Path, binary: Path) -> None:
    output = subprocess.run(
        [str(readelf), "--sections", "--wide", str(binary)],
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    if ".symtab" not in output:
        raise RuntimeError(f"BOLT input lacks symbols: {binary}")
    if ".real.text" not in output and ".rel.text" not in output:
        raise RuntimeError(
            f"BOLT input lacks text relocations: {binary}; "
            "link with `-Clink-arg=-Wl,--emit-relocs`"
        )


def validate_security(readelf: Path, binary: Path) -> None:
    output = subprocess.run(
        [str(readelf), "--program-headers", "--wide", str(binary)],
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    stack = next((line for line in output.splitlines() if "GNU_STACK" in line), "")
    if not stack or "E" in stack.split()[-2]:
        raise RuntimeError("BOLT executable has a missing or executable GNU stack")
    if "GNU_RELRO" not in output:
        raise RuntimeError("BOLT executable no longer has GNU RELRO protection")


def version(binary: Path) -> str:
    return subprocess.run(
        [str(binary), "--version"],
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def temporary_path(directory: Path, prefix: str, suffix: str) -> Path:
    with tempfile.NamedTemporaryFile(
        dir=directory, prefix=prefix, suffix=suffix, delete=False
    ) as temporary:
        return Path(temporary.name)


def rebuild_wheel(wheel: Path, binary: Path) -> Path:
    binary_data = binary.read_bytes()
    staged = temporary_path(wheel.parent, f".{wheel.name}.", ".tmp")
    try:
        with zipfile.ZipFile(wheel) as original:
            members = original.infolist()
            names = [member.filename for member in members if not member.is_dir()]
            if len(names) != len(set(names)):
                raise RuntimeError(f"Wheel has duplicate entries: {wheel}")

            records = [name for name in names if name.endswith(".dist-info/RECORD")]
            executables = []
            for member in members:
                if member.is_dir() or PurePosixPath(member.filename).name != "ruff":
                    continue
                with original.open(member) as stream:
                    if stream.read(len(ELF_MAGIC)) == ELF_MAGIC:
                        executables.append(member.filename)
            if len(records) != 1 or len(executables) != 1:
                raise RuntimeError(f"Wheel must have one RECORD and Ruff ELF: {wheel}")

            record_name = records[0]
            binary_name = executables[0]
            content = {
                member.filename: (
                    binary_data
                    if member.filename == binary_name
                    else original.read(member)
                )
                for member in members
                if not member.is_dir() and member.filename != record_name
            }
            rows = [
                [name, "", ""]
                if name == record_name
                else [name, record_hash(content[name]), str(len(content[name]))]
                for name in names
            ]
            record_stream = io.StringIO(newline="")
            csv.writer(record_stream, lineterminator="\n").writerows(rows)
            content[record_name] = record_stream.getvalue().encode("utf-8")

            with zipfile.ZipFile(staged, mode="w") as rebuilt:
                rebuilt.comment = original.comment
                for member in members:
                    rebuilt.writestr(
                        copy.copy(member),
                        b"" if member.is_dir() else content[member.filename],
                        compress_type=member.compress_type,
                        compresslevel=(
                            6 if member.compress_type == zipfile.ZIP_DEFLATED else None
                        ),
                    )

        validate_wheel(staged, binary_name, binary_data, names)
        print(f"Staged BOLT-optimized wheel: {wheel.name}", flush=True)
        return staged
    except BaseException:
        staged.unlink(missing_ok=True)
        raise


def record_hash(data: bytes) -> str:
    encoded = base64.urlsafe_b64encode(hashlib.sha256(data).digest())
    return f"sha256={encoded.rstrip(b'=').decode('ascii')}"


def validate_wheel(
    wheel: Path, binary_name: str, binary_data: bytes, expected_names: list[str]
) -> None:
    with zipfile.ZipFile(wheel) as archive:
        names = [
            member.filename for member in archive.infolist() if not member.is_dir()
        ]
        if names != expected_names:
            raise RuntimeError("Repacked wheel changed its archive members")
        record_name = next(name for name in names if name.endswith(".dist-info/RECORD"))
        records = list(csv.reader(io.StringIO(archive.read(record_name).decode())))
        if len(records) != len(names) or {row[0] for row in records} != set(names):
            raise RuntimeError("Repacked wheel has incomplete RECORD entries")
        for row in records:
            if len(row) != 3:
                raise RuntimeError("Repacked wheel has malformed RECORD entries")
            name, digest, size = row
            if name == record_name:
                if digest or size:
                    raise RuntimeError("Wheel RECORD must not hash itself")
                continue
            data = archive.read(name)
            if digest != record_hash(data) or size != str(len(data)):
                raise RuntimeError(f"Wheel RECORD does not match {name}")

        if archive.read(binary_name) != binary_data:
            raise RuntimeError("Repacked wheel does not contain the BOLT executable")
        if not (archive.getinfo(binary_name).external_attr >> 16) & 0o111:
            raise RuntimeError("Repacked Ruff wheel lost executable permissions")


if __name__ == "__main__":
    try:
        main()
    except (
        OSError,
        RuntimeError,
        subprocess.CalledProcessError,
        zipfile.BadZipFile,
    ) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
