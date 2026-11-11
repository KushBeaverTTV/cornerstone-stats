#!/usr/bin/python3
"""Static checks on the QML and JS: run with `python3 tests/qml_check.py`.

- Every file parses (qmlformat, when Qt's tools are installed).
- The shell's Color singleton is always reached as Commons.Color: Qt 6.12
  added a QtQuick `Color` that shadows it, and a bare Color.popups.text is
  undefined there, which paints text black.
- No em dash in anything the user reads.
- Every bare Text sets textFormat: Text.PlainText (kit Labels do it for you).
"""
from pathlib import Path
import re
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
QML = sorted(p for p in ROOT.glob("**/*.qml") if not {".claude", "target"} & set(p.relative_to(ROOT).parts))
JS = sorted((ROOT / "lib").glob("*.js"))
BARE_COLOR = re.compile(r"(?<![\w.])Color\s*\.")
problems = []


def code(line):
    return line.split("//", 1)[0]


qmlformat = shutil.which("qmlformat") or ("/usr/lib/qt6/bin/qmlformat" if Path("/usr/lib/qt6/bin/qmlformat").exists() else None)
for path in QML:
    if qmlformat:
        run = subprocess.run([qmlformat, str(path)], capture_output=True, text=True)
        if run.returncode != 0:
            problems.append(f"{path.relative_to(ROOT)}: does not parse: {run.stderr.strip().splitlines()[:1]}")
    lines = path.read_text().splitlines()
    for number, line in enumerate(lines, 1):
        if BARE_COLOR.search(code(line)):
            problems.append(f"{path.relative_to(ROOT)}:{number}: bare Color, use Commons.Color")
    # A plain `Text {` block must say PlainText within its first lines.
    for number, line in enumerate(lines, 1):
        if re.match(r"\s*(component \w+: )?Text \{", line):
            window = "\n".join(lines[number:number + 12])
            if "PlainText" not in window:
                problems.append(f"{path.relative_to(ROOT)}:{number}: Text without textFormat: Text.PlainText")

for path in QML + JS + [ROOT / "manifest.json", ROOT / "README.md"]:
    if path.exists() and "—" in path.read_text():
        problems.append(f"{path.relative_to(ROOT)}: contains an em dash")

for p in problems:
    print(p)
print(f"{len(QML)} QML and {len(JS)} JS files checked, {len(problems)} problems")
sys.exit(1 if problems else 0)
