# Downloaded dictionaries

These Hunspell dictionaries are **not bundled**. The app downloads them on
demand (Preferences > Spell check) from
[LibreOffice/dictionaries](https://github.com/LibreOffice/dictionaries),
pinned to commit `67032ee957631908098a72edf48dc81cac27e369`, and checks each
file against the SHA-256 hard-coded in `crates/lemmata-spell/src/remote.rs`.
Polish and Greek (ISO-8859 upstream) are recoded to UTF-8 after the check;
Danish entries with a malformed numeric flag and Turkish's affix class `0` are
adjusted for spellbook. Files land in `<app data>/dictionaries/<code>/`.

All licences below allow redistribution; because nothing is shipped, they only
matter to whoever downloads. Per-dictionary READMEs are in the upstream repo.

| Code | Language | Source files | Size | Licence |
|---|---|---|---|---|
| `es` | Español | `es/es_ES.aff/.dic` | 1 MB | GPL-3.0+ OR LGPL-3.0+ OR MPL-1.1+ |
| `it` | Italiano | `it_IT/it_IT.aff/.dic` | 1 MB | GPL-3.0 |
| `pt-PT` | Português (Portugal) | `pt_PT/pt_PT.aff/.dic` | 1 MB | GPL-2.0 OR LGPL-2.1 OR MPL-1.1 |
| `pt-BR` | Português (Brasil) | `pt_BR/pt_BR.aff/.dic` | 5 MB | LGPL-3.0 OR MPL-1.1 |
| `nl` | Nederlands | `nl_NL/nl_NL.aff/.dic` | 2 MB | BSD-3-Clause OR CC-BY-3.0 |
| `pl` | Polski | `pl_PL/pl_PL.aff/.dic` | 5 MB | GPL OR LGPL OR MPL OR Apache-2.0 OR CC-BY-SA |
| `sv` | Svenska | `sv_SE/dictionaries/sv_SE.aff/.dic` | 2 MB | LGPL-3.0 |
| `da` | Dansk | `da_DK/da_DK.aff/.dic` | 3 MB | GPL-2.0 OR LGPL-2.1 OR MPL-1.1 |
| `nb` | Norsk bokmål | `no/nb_NO.aff/.dic` | 8 MB | CC-BY-4.0 (word list, attribution to Nasjonalbiblioteket) AND GPL-2.0 (affix rules) |
| `cs` | Čeština | `cs_CZ/cs_CZ.aff/.dic` | 3 MB | GPL-2.0 |
| `ru` | Русский | `ru_RU/ru_RU.aff/.dic` | 3 MB | BSD-style (Lebedev) |
| `uk` | Українська | `uk_UA/uk_UA.aff/.dic` | 8 MB | MPL-1.1 |
| `tr` | Türkçe | `tr_TR/tr_TR.aff/.dic` | 36 MB | MPL-2.0 |
| `el` | Ελληνικά | `el_GR/el_GR.aff/.dic` | 9 MB | MPL-1.1 OR GPL-2.0 OR LGPL-2.1 |

Norwegian Bokmål: the word list is derived from Ordbanken
(Nasjonalbiblioteket, CC BY 4.0) and the 2018-2024 Bokmålsordboka revision
(CLARIN PUB +BY); credit them when redistributing.
