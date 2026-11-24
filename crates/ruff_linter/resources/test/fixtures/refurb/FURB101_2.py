# FURB101
with open("file.txt", encoding="utf-8") as f:
    _ = f.read()
f = object()
print(f)

# See: https://github.com/astral-sh/ruff/issues/21483
with open("file.txt", encoding="utf-8") as f:
    _ = f.read()
print(f.mode)

# Rebinding in a later `with ... as config_file` should not suppress this one.
with open("config.yaml", encoding="utf-8") as config_file:
    config_raw = config_file.read()

if "tts:" in config_raw:
    try:
        with open("config.yaml", "w", encoding="utf-8") as config_file:
            config_file.write(config_raw.replace("tts:", "google_translate:"))
    except OSError:
        pass
