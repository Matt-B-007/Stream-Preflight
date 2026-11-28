#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Makes assets/fonts/lucide.ttf (only the icons Stream Preflight uses) and src/icons.rs.

Lucide comes from npm:  npm install lucide-static@0.555.0
Then, in the project folder:
    python3 tools/lucide-subset.py node_modules/lucide-static/font
Needs fontTools (Arch: sudo pacman -S python-fonttools).
Add an icon: put its Lucide name in ICONS and run the script again.
"""

import json
import pathlib
import sys

from fontTools import subset

ICONS = [
    "house", "plane-takeoff", "gauge", "radio", "history", "settings",
    "circle-question-mark", "search", "keyboard", "circle-arrow-up", "undo-2", "x",
    "sparkles", "info", "bug", "map", "sun", "moon", "monitor", "external-link",
    "folder-open", "copy", "save", "rotate-ccw", "refresh-cw", "triangle-alert",
    "circle-check", "gamepad-2", "video", "command", "list-checks", "wrench",
    "book-open", "power", "bell", "shield-check", "sliders-horizontal", "trash-2",
    "file-text", "chevron-right", "arrow-right", "play", "square", "cpu", "wifi",
    "log-out", "circle-x", "circle-alert",
]


def main() -> None:
    src = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "node_modules/lucide-static/font")
    root = pathlib.Path(__file__).resolve().parent.parent
    info = json.loads((src / "info.json").read_text())
    codes = {}
    for name in ICONS:
        hexcode = info[name]["encodedCode"].lstrip("\\")
        codes[name] = int(hexcode, 16)
    out = root / "assets" / "fonts" / "lucide.ttf"
    opts = subset.Options()
    opts.notdef_outline = True
    font = subset.load_font(str(src / "lucide.ttf"), opts)
    sub = subset.Subsetter(opts)
    sub.populate(unicodes=list(codes.values()))
    sub.subset(font)
    subset.save_font(font, str(out), opts)
    lines = [
        "// SPDX-License-Identifier: GPL-3.0-or-later",
        "//! Lucide icons (ISC licence, assets/fonts/Lucide-LICENSE.txt), drawn from",
        "//! assets/fonts/lucide.ttf. Made by tools/lucide-subset.py: don't edit by hand.",
        "#![allow(dead_code)]",
        "",
    ]
    for name, code in codes.items():
        const = name.upper().replace("-", "_")
        lines.append(f"pub const {const}: &str = \"\\u{{{code:04x}}}\";")
    (root / "src" / "icons.rs").write_text("\n".join(lines) + "\n")
    print(f"{len(codes)} icons -> {out} ({out.stat().st_size} bytes) and src/icons.rs")


if __name__ == "__main__":
    main()
