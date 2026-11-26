#!/usr/bin/env python3
"""Materialize the deterministic Issue #4968 GPOS kerning smoke faces.

The space variant adds pairs next to a space, which Hancom does not kern.
"""

from __future__ import annotations

import hashlib
import sys
from pathlib import Path

from font_kerning_boundary import synthetic_font


ROOT = Path(__file__).resolve().parents[1]
FONTS = ROOT / "tests" / "fixtures" / "fonts"
FIXTURES = (
    (
        "RHWPExactKerningSmoke.ttf",
        "pos A V -80; pos T o -40;",
        "775667d1980cd734e331f01e9390e02191bc35d669325291c842968cb0a4a9fc",
    ),
    (
        "RHWPExactKerningSpaceSmoke.ttf",
        "pos A V -80; pos T o -40; pos space A -80; pos V space -80;",
        "abc65b67a74d5a5620ce235dc09543e66a6378077fbf320e920db3797d5345ff",
    ),
)


def fixture_bytes(pairs: str, expected_sha256: str) -> bytes:
    data = synthetic_font(gpos=True, legacy=False, pairs=pairs)
    digest = hashlib.sha256(data).hexdigest()
    if digest != expected_sha256:
        raise RuntimeError(f"exact kerning fixture drift: {digest}")
    return data


def main() -> int:
    FONTS.mkdir(parents=True, exist_ok=True)
    for name, pairs, expected_sha256 in FIXTURES:
        output = FONTS / name
        data = fixture_bytes(pairs, expected_sha256)
        output.write_bytes(data)
        print(f"{output.relative_to(ROOT)} {len(data)} {expected_sha256}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
