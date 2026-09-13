# MacType legacy INI compatibility corpus

These 70 INI files are copied without modification from the `ini/` directory of
[`luantu/MacType`](https://github.com/luantu/MacType) at commit
`f3e926f75fe134ab1438b792925c082679c715d3` (2024-12-30).

The source repository distributes MacType and these profiles under GPL-3.0. This
repository is also GPL-3.0. The root `LICENSE` file contains the applicable
license text.

The official [`snowie2000/mactype`](https://github.com/snowie2000/mactype)
source tree contains no `ini/` directory or `.ini` files at commit
`05052e88c7ce134f93b66db95132284a1ed10de7`. The official 2025.6.9 installer
could not be inspected without executing an untrusted third-party extractor, so
this pinned public Chinese-community distribution is used as a real-world
compatibility corpus instead of being misrepresented as an official upstream
fixture set.

Corpus characteristics:

- 70 INI files, including nested profile collections
- 48 UTF-16 LE files with BOM
- 16 UTF-8 files
- 6 BOM-less GBK/GB18030 files with CRLF line endings

The six legacy Chinese files and their SHA-256 hashes are:

| File | SHA-256 |
| --- | --- |
| `CRT.ini` | `4fd96a61c16a3ac463b08298aa2f69b62859a86a9a457c3abf861c9bf24601fe` |
| `CandyTypeSharpFix.ini` | `6e53d3424f08f71f2b8c94e7dd5f9bf3072bd28bffaa23141514d5bf9ccaa2b6` |
| `LCD.ini` | `6a0bf9dfcbce4967be5b8ec7f53f30a8e4d4ebe7857eecca586b71c7e581a67b` |
| `luantu - 副本.ini` | `5d027c6d5abdb1546c589196a93649d05e863549c0dab2f94e275248c942369a` |
| `luantu.ini` | `ff4c14e3f4fe10ad96351f5be315b6fdd3b5be6a8faa2eba838ccff82f639135` |
| `new.ini` | `b21cedc1fa066262ee425591b666b0aa644be6df5e2c76d9798df65b1e96a91f` |

Tests intentionally assert both byte-for-byte unchanged round trips and the
codec selected for these six files. A byte-only assertion is insufficient
because the editor preserves untouched legacy lines and can therefore reproduce
the original bytes even after choosing the wrong decoder.
