#!/usr/bin/env python3
"""Generate semantic-tinted Android vectors from the approved Iconsax SVG source.

Run from the repository root: python3 mobile/android/tools/generate_approved_vectors.py
The generated paths use black plus source opacity because Compose Icon supplies the semantic tint.
"""
from pathlib import Path
from xml.etree import ElementTree as ET

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "backend/resources/icons/iconsax"
OUTPUT = ROOT / "mobile/android/app/src/main/res/drawable"
ICONS = [
    "alert",
    "archive",
    "arrow-down",
    "arrow-down-circle",
    "arrow-left",
    "arrow-left-circle",
    "arrow-right",
    "arrow-right-circle",
    "arrow-up",
    "arrow-up-circle",
    "arrow-up-right",
    "attachment",
    "bell",
    "bell-off",
    "bold",
    "bolt",
    "book",
    "brush",
    "calendar",
    "check",
    "chevron-down",
    "chevron-up",
    "clock",
    "code",
    "code-block",
    "color",
    "columns",
    "copy",
    "credit-card",
    "crop",
    "dashboard",
    "download",
    "edit",
    "eye",
    "eye-off",
    "file",
    "filter",
    "flip",
    "folder",
    "globe",
    "heart",
    "help",
    "home",
    "image",
    "info",
    "italic",
    "key",
    "language",
    "link",
    "list",
    "loader",
    "lock",
    "login",
    "logout",
    "maximize",
    "menu",
    "mobile",
    "monitor",
    "moon",
    "move",
    "music",
    "note",
    "notification",
    "number",
    "ordered-list",
    "plans",
    "plus",
    "quote",
    "redo",
    "relationship",
    "reports",
    "rotate-left",
    "rotate-right",
    "search",
    "select",
    "settings",
    "shield",
    "slash",
    "sort",
    "storage",
    "strikethrough",
    "sun",
    "table",
    "tag",
    "text",
    "transfers",
    "trash",
    "underline",
    "undo",
    "unlock",
    "updates",
    "upload",
    "user",
    "users",
    "video",
    "x",
    "zoom-in",
    "zoom-out",
    "end-to-end-encrypted",
    "fast-and-simple",
    "laptop",
    "terminal",
]
ANDROID = "http://schemas.android.com/apk/res/android"

def emit_path(node: ET.Element, inherited_alpha: float, paths: list[str]) -> None:
    alpha = inherited_alpha * float(node.attrib.get("opacity", "1"))
    if node.tag.endswith("path"):
        attributes = [f'android:pathData="{node.attrib["d"]}"']
        if "fill" in node.attrib and node.attrib["fill"] != "none": attributes.append('android:fillColor="#000000"')
        if "stroke" in node.attrib: attributes.extend(['android:strokeColor="#000000"', f'android:strokeWidth="{node.attrib.get("stroke-width", "1")}"'])
        for source, target in (("stroke-linecap", "strokeLineCap"), ("stroke-linejoin", "strokeLineJoin")):
            if source in node.attrib: attributes.append(f'android:{target}="{node.attrib[source]}"')
        if alpha != 1: attributes.append(f'android:strokeAlpha="{alpha:g}" android:fillAlpha="{alpha:g}"')
        paths.append("    <path " + " ".join(attributes) + "/>\n")
    for child in node:
        if not child.tag.endswith("defs"):
            emit_path(child, alpha, paths)

for source_name in ICONS:
    output_name = source_name.replace("-", "_")
    root = ET.parse(SOURCE / f"{source_name}.svg").getroot()
    paths: list[str] = []
    emit_path(root, 1, paths)
    target = OUTPUT / f"ic_approved_{output_name}.xml"
    target.write_text('<vector xmlns:android="http://schemas.android.com/apk/res/android" android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24">\n' + "".join(paths) + "</vector>\n")
