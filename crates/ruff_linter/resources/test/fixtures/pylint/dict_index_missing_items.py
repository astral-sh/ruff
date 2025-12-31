ORCHESTRA = {
    "violin": "strings",
    "oboe": "woodwind",
    "tuba": "brass",
    "gong": "percussion",
}

# Errors
for instrument in ORCHESTRA:
    print(f"{instrument}: {ORCHESTRA[instrument]}")

for instrument in ORCHESTRA:
    ORCHESTRA[instrument]

for instrument in ORCHESTRA.keys():
    print(f"{instrument}: {ORCHESTRA[instrument]}")

for instrument in ORCHESTRA.keys():
    ORCHESTRA[instrument]

for instrument in (temp_orchestra := {"violin": "strings", "oboe": "woodwind"}):
    print(f"{instrument}: {temp_orchestra[instrument]}")

for instrument in (temp_orchestra := {"violin": "strings", "oboe": "woodwind"}):
    temp_orchestra[instrument]

# # OK
for instrument, section in ORCHESTRA.items():
    print(f"{instrument}: {section}")

for instrument, section in ORCHESTRA.items():
    section

for instrument, section in (
    temp_orchestra := {"violin": "strings", "oboe": "woodwind"}
).items():
    print(f"{instrument}: {section}")

for instrument, section in (
    temp_orchestra := {"violin": "strings", "oboe": "woodwind"}
).items():
    section

for instrument in ORCHESTRA:
    ORCHESTRA[instrument] = 3


# Shouldn't trigger for non-dict types
items = {1, 2, 3, 4}
for i in items:
    items[i]

items = [1, 2, 3, 4]
for i in items:
    items[i]


# A case with multiple uses of the value to show off the secondary annotations
for instrument in ORCHESTRA:
    data = json.dumps(
        {
            "instrument": instrument,
            "section": ORCHESTRA[instrument],
        }
    )

    print(f"saving data for {instrument} in {ORCHESTRA[instrument]}")

    with open(f"{instrument}/{ORCHESTRA[instrument]}.txt", "w") as f:
        f.write(data)


# This should still suppress the error
for (  # noqa: PLC0206
    instrument
) in ORCHESTRA:
    print(f"{instrument}: {ORCHESTRA[instrument]}")
