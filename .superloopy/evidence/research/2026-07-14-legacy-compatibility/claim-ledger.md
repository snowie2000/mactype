# Claim ledger

| Claim | Risk | Domains | Counter-search | Primary | Status |
| --- | --- | --- | --- | --- | --- |
| The upstream git tree contains an `ini/` corpus | normal | github.com | searched current tree, all refs, and Git object history | Git tree at `05052e8` | refuted |
| A pinned public Chinese-community corpus represents real deployed profiles | normal | github.com, local bytes | fixed commit, recursive count, codec scan, hashes | `luantu/MacType@f3e926f` | verified |
| Unchanged byte round-trip alone proves correct decoding | high | Rust implementation, corpus | traced original-line byte reuse and asserted selected codec | `profile/codec.rs`, `profile.rs` | refuted |
| The six BOM-less legacy files should select GB18030 | high | corpus bytes, WHATWG codecs | compared candidate decoders and edited CJK values | six SHA-256-pinned fixtures | verified |
| A service query failure means MacType is absent | high | Win32 SCM | enumerated OpenService/QueryServiceConfig/QueryServiceStatus errors | Microsoft SCM documentation | refuted |
| The official Tauri plugin strictly prevents simultaneous cold starts | high | plugin source, Win32 | traced mutex creation to IPC HWND creation | plugin `single-instance` 2.4.3 source | refuted |
| The main GUI can safely run elevated while keeping single-instance behavior | high | plugin source, Windows MIC/UIPI | tested both integrity directions by code-path analysis | Microsoft MIC/UIPI documentation | refuted |
