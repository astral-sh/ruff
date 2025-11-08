from __future__ import annotations

import json
import subprocess

from pathlib import Path
from datetime import datetime


def get_commit_hash() -> str:
    try:
        commit_hash = subprocess.check_output(["git", "rev-parse", "HEAD"]).decode(
            "ascii"
        )
    except subprocess.CalledProcessError:
        commit_hash = "Unknown"
    return commit_hash


class ErrorCountSummary:
    def __init__(
        self,
        date_time: datetime = datetime.now(),
        error_counts: dict[str, int] = dict(),
        commit_hash: str = get_commit_hash(),
    ):
        self.commit_hash = commit_hash
        self.date_time: datetime = date_time
        self.error_counts: dict[str, int] = error_counts

    def write_json(self, file_path: Path) -> None:
        dictionary = {
            "commit_hash": self.commit_hash,
            "date_time": self.date_time.strftime("%Y-%m-%d %H:%M:%S"),
            "error_counts": self.error_counts,
        }
        with open(file_path, "w") as f:
            json.dump(dictionary, f)

    @classmethod
    def from_json(cls, file_path: Path) -> ErrorCountSummary:
        with open(file_path, "r") as f:
            data = json.load(f)

        error_count_summary = cls(
            date_time=datetime.strptime(data["date_time"], "%Y-%m-%d %H:%M:%S"),
            error_counts=data["error_counts"],
            commit_hash=data["commit_hash"],
        )

        return error_count_summary

    def _print_row(self, items: list, cell_widths: list[int]) -> None:
        row = "|"
        for item, width in zip(items, cell_widths):
            row += f" {str(item):<{width}} |"
        print(row)

    def _print_line(self, cell_widths: list[int]) -> None:
        row = "+"
        for width in cell_widths:
            row += "-" * (width + 1) + "-+"
        print(row)

    def _print_title(self, title: str, cell_widths: list[int]):
        width = sum(cell_widths) + 3 * (len(cell_widths) - 1)
        print("+-" + "-" * width + "-+")
        print(f"| {title:^{width}} |")

    def print_comparison(self, other: ErrorCountSummary) -> None:
        if self.date_time < other.date_time:
            old = self
            new = other
        else:
            old = other
            new = self

        widths = [15, 20, 20, 20]

        self._print_title("Error Count Summary", widths)
        self._print_line(widths)
        self._print_row(
            [
                "",
                f"Old ({old.commit_hash[:8]})",
                f"New ({new.commit_hash[:8]})",
                "Difference",
            ],
            widths,
        )
        self._print_line(widths)

        old_datetime = old.date_time.strftime("%Y-%m-%d %H:%M:%S")
        new_datetime = new.date_time.strftime("%Y-%m-%d %H:%M:%S")
        datetime_difference = str(new.date_time - old.date_time)
        self._print_row(
            ["datetime", old_datetime, new_datetime, datetime_difference], widths
        )

        all_benchmark_names = list(old.error_counts.keys()) + list(
            new.error_counts.keys()
        )
        unique_benchmark_names = sorted(set(all_benchmark_names))

        for name in unique_benchmark_names:
            old_count = str(old.error_counts.get(name, "-"))
            new_count = str(new.error_counts.get(name, "-"))
            if name not in old.error_counts or name not in new.error_counts:
                count_difference = "-"
            else:
                count_difference = new.error_counts[name] - old.error_counts[name]

            self._print_row([name, old_count, new_count, count_difference], widths)

        self._print_line(widths)
