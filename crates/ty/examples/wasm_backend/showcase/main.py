TITLE = "ty wasm release"
SCALE = 1.5


def triangular(value: int) -> int:
    if value <= 1:
        return value
    return value + triangular(value - 1)


def feature_bonus(feature_count: int, weight: int = 2) -> int:
    return feature_count * weight + triangular(3)


class ReleaseReport:
    def __init__(self, name: str, base_score: int, multiplier: float = 1.5) -> None:
        self.name = name
        self.score = base_score
        self.multiplier = multiplier

    def apply_bonus(self, bonus: int = 0) -> int:
        return self.score + bonus

    def confidence(self, factor: float = 2.0) -> float:
        return self.multiplier * factor


feature_scores = [3, 4, 5]
feature_names = ["parser", "types", "backend"]
areas = {"frontend": 3, "backend": 0}
tags = ("typed", "wasm")

feature_scores[1] = 7
feature_names[0] = "compiler"
feature_scores.append(9)
areas["backend"] = feature_bonus(feature_count=2, weight=3)

raw_score = 0
for score in feature_scores:
    raw_score += score

area_score = 0
for area in areas:
    area_score += areas[area]

tag_letters = 0
for tag in tags:
    tag_letters += len(tag)

indexed_score = 0
for index in range(len(feature_scores)):
    indexed_score += feature_scores[index]

report = ReleaseReport(name="ty", base_score=raw_score, multiplier=SCALE)
report.score = report.apply_bonus(bonus=areas["backend"])

status = "stable"
if TITLE < "zzzz":
    status = "ready"
if feature_names[0] != feature_names[1]:
    status = status + "-distinct"

print(TITLE)
print(report.name + ":" + status)
print(report.score)
print(report.confidence())
print(feature_names[0])
print(area_score)
print(tag_letters)
print(indexed_score)
