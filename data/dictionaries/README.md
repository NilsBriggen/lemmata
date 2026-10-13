# Spelling dictionaries

Hunspell dictionaries used by `crates/lemmata-spell`, shipped with the app.
Each folder keeps the upstream licence file.

| Folder | Source | Licence |
|---|---|---|
| `en` | [dictionary-en](https://github.com/wooorm/dictionaries) 4.0.0 (SCOWL) | MIT AND BSD |
| `en-GB` | dictionary-en-gb 3.0.0 (SCOWL) | MIT AND BSD |
| `de` | dictionary-de 3.0.0 (igerman98) | GPL-2.0 OR GPL-3.0 |
| `fr` | dictionary-fr 3.0.0 (Dicollecte 7.5) | MPL-2.0 |

Swiss German (`de-CH`) uses the `de` dictionary and accepts "ss" wherever
standard German writes "ß"; the upstream de-CH package still contains ß forms.
