# Visual QA

Status: PASS for the implemented phase-one surface.

## Captured surfaces

Overview, profiles, and diagnostics were built for production and opened in real Chromium at 390 by 844, 768 by 900, and 1280 by 800. All nine cases passed with no page exception, console error, renderer crash, missing readiness marker, or page-level horizontal overflow. The nine PNG files are under `screenshots/` and were also inspected directly.

The desktop profile view keeps a line-separated editor above a fixed preview. The mobile view stacks controls and keeps the section index in its own horizontal scroller without causing page-level overflow. The official MacType installer icon is visible at native UI size.

## Anti-slop preflight

- [x] Zero visible em dash or en dash characters.
- [x] No tracked uppercase eyebrow labels.
- [x] No purple glow, gradient, beige-and-brass palette, glass, or bento layout.
- [x] Native Windows typography is intentional and documented in `DESIGN.md`.
- [x] Color, shape, and theme locks hold across all three views.
- [x] No fake screenshot or decorative statistic is used. The preview is explicitly labeled as a placeholder until Helper rendering is connected.
- [x] Copy contains no marketing cliches, fake precision, generic people, or fake version footer.
- [x] Motion uses opacity and transform only and respects reduced motion.
- [x] The design compliance script found zero undeclared color or spacing violations.
- [x] Hover, active, focus, disabled-capable controls, loading-safe startup, incomplete installation, and warning states are represented.
- [x] No page-level horizontal scroll at 390, 768, or 1280 pixels.

No UX was weakened to pass a check.
