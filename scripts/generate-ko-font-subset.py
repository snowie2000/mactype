#!/usr/bin/env python3
"""Regenerate the bundled Korean UI webfont subset.

Chromium and WebView2 never select Malgun Gothic's Bold cut (see
control-center/src/styles/tokens.css), so the Korean UI ships a subset of
Pretendard Variable that carries real weights for exactly the glyphs the
UI can produce: every non-ASCII character in ko.json plus non-ASCII
literals in control-center/src TypeScript sources. Latin stays on Segoe UI
Variable, and glyphs outside the subset fall through to Malgun Gothic.

Source font (not committed):
  Pretendard Variable v1.3.9, SIL OFL 1.1
  https://cdn.jsdelivr.net/gh/orioncactus/pretendard@v1.3.9/packages/pretendard/dist/public/variable/PretendardVariable.ttf

Usage:
  python scripts/generate-ko-font-subset.py path/to/PretendardVariable.ttf

Requires fonttools (pip install fonttools brotli).
"""

import hashlib
import json
import sys
from pathlib import Path

from fontTools.subset import main as subset_main

ROOT = Path(__file__).resolve().parents[1]
KO_JSON = ROOT / "control-center" / "src" / "i18n" / "ko.json"
SOURCE_DIR = ROOT / "control-center" / "src"
OUT_DIR = ROOT / "control-center" / "src" / "assets" / "fonts"
GLYPHS_FILE = OUT_DIR / "ko-glyphs.txt"
WOFF2_FILE = OUT_DIR / "pretendard-ko-ui.woff2"
SOURCE_SHA256 = "3090ccde0442bb347aa7685d9ba8b17436a60682df6e8f92a9a670de14056e22"


def unique_ui_characters() -> set[str]:
    characters: set[str] = set()
    catalog = json.loads(KO_JSON.read_text(encoding="utf-8"))
    for value in catalog.values():
        characters.update(value)
    for pattern in ("**/*.ts", "**/*.tsx"):
        for source in SOURCE_DIR.glob(pattern):
            characters.update(source.read_text(encoding="utf-8"))
    return {ch for ch in characters if ord(ch) > 0x7E and not ch.isspace()}


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    source = Path(sys.argv[1])
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    if digest != SOURCE_SHA256:
        raise SystemExit(
            f"Source font hash mismatch.\n  expected {SOURCE_SHA256}\n  actual   {digest}\n"
            "Update SOURCE_SHA256 only when intentionally moving to a new Pretendard release."
        )

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    glyphs = "".join(sorted(unique_ui_characters()))
    GLYPHS_FILE.write_text(glyphs + "\n", encoding="utf-8")

    subset_main([
        str(source),
        f"--text-file={GLYPHS_FILE}",
        "--unicodes=U+0020,U+00A0",
        "--layout-features=*",
        "--name-IDs=0,1,2,3,4,6,13,14",
        "--flavor=woff2",
        f"--output-file={WOFF2_FILE}",
    ])

    print(f"{GLYPHS_FILE.relative_to(ROOT)}: {len(glyphs)} glyphs")
    print(f"{WOFF2_FILE.relative_to(ROOT)}: {WOFF2_FILE.stat().st_size:,} bytes")


if __name__ == "__main__":
    main()
