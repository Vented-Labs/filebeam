#!/usr/bin/env python3
"""Generate the desktop client's static font instances.

GPUI's Linux text system renders a variable font at its default weight only, so the
desktop bundles static instances. They are derived from the variable fonts tracked
for the Android client and subset to the scripts the interface uses.

Requires fontTools (`pip install fonttools`). Run from any directory:

    python3 scripts/desktop/generate-fonts.py [--check]
"""
import argparse
import hashlib
import io
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parents[2]
SOURCES = ROOT / "mobile/android/app/src/main/res/font"
OUTPUT = ROOT / "desktop/assets/fonts"

# (output file, source, family, style, weight, extra axis locations)
INSTANCES = [
    ("Inter-Regular.ttf", "inter_variable.ttf", "Inter", "Regular", 400, {"opsz": 14}),
    ("Inter-Medium.ttf", "inter_variable.ttf", "Inter", "Medium", 500, {"opsz": 14}),
    ("Inter-SemiBold.ttf", "inter_variable.ttf", "Inter", "SemiBold", 600, {"opsz": 14}),
    ("Inter-Bold.ttf", "inter_variable.ttf", "Inter", "Bold", 700, {"opsz": 14}),
    ("InterDisplay-SemiBold.ttf", "inter_variable.ttf", "Inter Display", "SemiBold", 600, {"opsz": 32}),
    ("InterDisplay-Bold.ttf", "inter_variable.ttf", "Inter Display", "Bold", 700, {"opsz": 32}),
    ("JetBrainsMono-Regular.ttf", "jetbrains_mono_variable.ttf", "JetBrains Mono", "Regular", 400, {}),
    ("JetBrainsMono-Medium.ttf", "jetbrains_mono_variable.ttf", "JetBrains Mono", "Medium", 500, {}),
]

# Kerning, contextual forms, mark positioning and tabular figures. Stylistic sets
# and code ligatures are left out: links and commands must read literally.
FEATURES = {
    "Inter": ["kern", "liga", "calt", "ccmp", "locl", "mark", "mkmk", "tnum", "case", "frac"],
    "Inter Display": ["kern", "liga", "calt", "ccmp", "locl", "mark", "mkmk", "tnum", "case", "frac"],
    "JetBrains Mono": ["kern", "ccmp", "locl", "mark", "mkmk", "zero"],
}

# Latin, Latin Extended, Greek, Cyrillic, punctuation, currency, arrows, common
# symbols and box drawing. Other scripts fall back to system fonts.
UNICODES = [
    (0x0020, 0x024F),
    (0x02B0, 0x02FF),
    (0x0300, 0x036F),
    (0x0370, 0x03FF),
    (0x0400, 0x052F),
    (0x1E00, 0x1EFF),
    (0x2000, 0x206F),
    (0x2070, 0x209F),
    (0x20A0, 0x20BF),
    (0x2100, 0x214F),
    (0x2150, 0x218F),
    (0x2190, 0x21FF),
    (0x2200, 0x22FF),
    (0x2300, 0x23FF),
    (0x2500, 0x259F),
    (0x25A0, 0x25FF),
    (0x2600, 0x26FF),
    (0x2700, 0x27BF),
    (0xFB00, 0xFB06),
    (0xFFFD, 0xFFFD),
]


def build(source: Path, family: str, style: str, weight: int, axes: dict) -> bytes:
    font = TTFont(source, recalcTimestamp=False)
    location = {"wght": weight, **axes}
    font = instancer.instantiateVariableFont(font, location, inplace=True)

    options = subset.Options()
    options.layout_features = FEATURES[family]
    options.name_IDs = [0, 13, 14]
    options.name_languages = [0x409]
    options.notdef_outline = True
    options.glyph_names = False
    options.hinting = False
    options.drop_tables += ["DSIG", "STAT", "meta"]
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=[code for low, high in UNICODES for code in range(low, high + 1)])
    subsetter.subset(font)

    bold = weight >= 700
    postscript = f"{family.replace(' ', '')}-{style}"
    names = {1: family, 2: style, 3: f"Filebeam;{postscript}", 4: f"{family} {style}", 6: postscript, 16: family, 17: style}
    table = font["name"]
    for name_id, value in names.items():
        table.setName(value, name_id, 3, 1, 0x409)
    os2 = font["OS/2"]
    os2.usWeightClass = weight
    os2.fsSelection = (os2.fsSelection & ~0b1100001) | (0b100000 if bold else 0b1000000 if style == "Regular" else 0)
    font["head"].macStyle = 1 if bold else 0

    buffer = io.BytesIO()
    font.save(buffer)
    return buffer.getvalue()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="fail when a tracked instance is stale")
    arguments = parser.parse_args()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    stale = []
    for name, source, family, style, weight, axes in INSTANCES:
        data = build(SOURCES / source, family, style, weight, axes)
        path = OUTPUT / name
        if arguments.check:
            if not path.is_file() or path.read_bytes() != data:
                stale.append(name)
            continue
        path.write_bytes(data)
        print(f"{name:28} {len(data) / 1024:7.1f} KiB  sha256 {hashlib.sha256(data).hexdigest()[:16]}")
    if stale:
        print(f"Stale desktop fonts: {', '.join(stale)}. Run scripts/desktop/generate-fonts.py.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
