# Third-party source notice

The independent package is produced from source in CI. No DLL or executable is copied from the closed Delphi distribution.

| Component | Pinned revision | Source and notice |
|---|---|---|
| MacType core and MacLoader | this repository revision | GPL-3.0-or-later; see `LICENSE` |
| FreeType | `ef771574d04721baf45a1b66bfb4692193603088` | <https://github.com/snowie2000/freetype>; FreeType License/GPL, see upstream `LICENSE.TXT` |
| Microsoft Detours | `d644ce94e8c7f7f5a31591577c78134ea3ac1fae` | <https://github.com/microsoft/Detours>; MIT, see upstream `LICENSE.md` |
| IniParser | `a457397ffa9d20e8df43e2c143c60da78c16c059` | <https://github.com/snowie2000/IniParser>; source dependency maintained by the MacType author; upstream has no separate license file |
| rewolf-wow64ext | `667359c7967249dd9d28d8f8cef65b60e7e2d963` | <https://github.com/snowie2000/rewolf-wow64ext>; source dependency; upstream has no separate license file |
| Pretendard (Korean UI glyph subset) | `v1.3.9` | <https://github.com/orioncactus/pretendard>; SIL OFL 1.1, see `control-center/src/assets/fonts/OFL-Pretendard.txt`; subset regenerated with `scripts/generate-ko-font-subset.py` |

The package uses the `Rel+Detours` core. It does not redistribute `EasyHK32.dll` or `EasyHK64.dll`; those external files are not required by this build. The Control Center icon provenance is recorded in `assets/README.md`.
