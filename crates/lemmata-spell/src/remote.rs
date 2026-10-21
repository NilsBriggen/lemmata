//! Dictionaries downloaded on demand from LibreOffice's dictionaries
//! repository, pinned to one commit. Each file's SHA-256 is hard-coded, so a
//! changed or tampered download is rejected.
//!
//! Licences (all allow redistribution; recorded per language below and in
//! `data/dictionaries/DOWNLOADED.md`). Nothing is bundled: the user's machine
//! fetches the files straight from GitHub.

use crate::Error;
use std::path::Path;
use std::time::Duration;

/// Master of https://github.com/LibreOffice/dictionaries when this list was made.
pub const COMMIT: &str = "67032ee957631908098a72edf48dc81cac27e369";

pub struct Remote {
    pub code: &'static str,
    pub label: &'static str,
    /// Repository path without extension; `.aff` and `.dic` are appended.
    pub path: &'static str,
    pub aff_sha256: &'static str,
    pub dic_sha256: &'static str,
    pub size_kb: u32,
    pub licence: &'static str,
}

impl Remote {
    pub fn urls(&self) -> [String; 2] {
        ["aff", "dic"].map(|ext| {
            format!(
                "https://raw.githubusercontent.com/LibreOffice/dictionaries/{COMMIT}/{}.{ext}",
                self.path
            )
        })
    }
}

pub const REMOTE: &[Remote] = &[
    Remote {
        code: "es",
        label: "Español",
        path: "es/es_ES",
        aff_sha256: "e73a9bf8e1383f4986a5dc9e2fbed49371c0c61f511c626d15586bd433c1cad9",
        dic_sha256: "6975dddec3d5d2c676069537bc67b4b5f786c65c5d4cf6703a82acf779ac9ec1",
        size_kb: 865,
        licence: "GPL-3.0+ OR LGPL-3.0+ OR MPL-1.1+",
    },
    Remote {
        code: "it",
        label: "Italiano",
        path: "it_IT/it_IT",
        aff_sha256: "951afaa19272f13555b8823e8bcf9ccf78f8fe1a07835bdfb912ab3e4d537c2b",
        dic_sha256: "bae1e3501dcd2a923669592493b3fde6c02aae7c7aab83bf5e5b49077e73dd64",
        size_kb: 1334,
        licence: "GPL-3.0",
    },
    Remote {
        code: "pt-PT",
        label: "Português (Portugal)",
        path: "pt_PT/pt_PT",
        aff_sha256: "975a209fcc892cb382fa5f34a28c391a39668661ce373ae071287809c5fcae24",
        dic_sha256: "e29ba2d7aa8a2ad43e9cb46ac6473064b661545c87002aea90e18899d98d3cc9",
        size_kb: 1545,
        licence: "GPL-2.0 OR LGPL-2.1 OR MPL-1.1",
    },
    Remote {
        code: "pt-BR",
        label: "Português (Brasil)",
        path: "pt_BR/pt_BR",
        aff_sha256: "21d8ad2a769a60e17e2b5ea4ef11d4d593a58b9e2a82d642ef82d6a4c5523865",
        dic_sha256: "a38bfb26b68ece2834e79fe83e48d5792652970ace12db89d1b9674bf9933183",
        size_kb: 5330,
        licence: "LGPL-3.0 OR MPL-1.1",
    },
    Remote {
        code: "nl",
        label: "Nederlands",
        path: "nl_NL/nl_NL",
        aff_sha256: "f0233d4f721f4661cf5f4d05ed2739549322bf3b6b66764b55a38257e1e16e6f",
        dic_sha256: "bc28af45307700a9927ad5719184da44dfd7eed4f707b8c1477f6d8a21b586a6",
        size_kb: 2476,
        licence: "BSD-3-Clause OR CC-BY-3.0",
    },
    Remote {
        code: "pl",
        label: "Polski",
        path: "pl_PL/pl_PL",
        aff_sha256: "82973651651aa930335c865b339b98db376ca3dbf3a661b70b9eeb71fdf41dca",
        dic_sha256: "c0848440599eb88e5aca500418d5f389e562ec2c157b63dbe39d354658ffba49",
        size_kb: 5381,
        licence: "GPL OR LGPL OR MPL OR Apache-2.0 OR CC-BY-SA",
    },
    Remote {
        code: "sv",
        label: "Svenska",
        path: "sv_SE/dictionaries/sv_SE",
        aff_sha256: "b721c9d44bee912feb182b601a1bc2ae3e7dffef660f4130cf2751867488a9dd",
        dic_sha256: "384a2126eff333f5f6f9790ae892554546f53948d2988c600397cb5ad6ce66e8",
        size_kb: 2308,
        licence: "LGPL-3.0",
    },
    Remote {
        code: "da",
        label: "Dansk",
        path: "da_DK/da_DK",
        aff_sha256: "ee4fe193d19b72aa64bc0d2b9bcdf44ee44f11d9b4e7dcf6da36fd0e91528561",
        dic_sha256: "298aabeca362ee2c71836c5c7748d22080a460bd7193e6fbd2471f27a70d0fd3",
        size_kb: 3884,
        licence: "GPL-2.0 OR LGPL-2.1 OR MPL-1.1",
    },
    Remote {
        code: "nb",
        label: "Norsk bokmål",
        path: "no/nb_NO",
        aff_sha256: "b9e828538222b3580d8adb4660d5bae9bc1fbe42e7c21fd3e01970efed96b4f8",
        dic_sha256: "aadaf9ea6b437b4edada54a2084e1c8dc00029addc6dfb4e15637f8e798dacc1",
        size_kb: 9135,
        licence: "CC-BY-4.0 (word list, attribution to Nasjonalbiblioteket) AND GPL-2.0 (affix rules)",
    },
    Remote {
        code: "cs",
        label: "Čeština",
        path: "cs_CZ/cs_CZ",
        aff_sha256: "7ecb20620ecd46ebd9c36f3f33e69dd4eda385cba5b2bb4e6bc396d910e297f7",
        dic_sha256: "d8e8c88c006fdae72dac8c85df11b0c99a773e05a4ab0fcbe92244876668ca74",
        size_kb: 3680,
        licence: "GPL-2.0",
    },
    Remote {
        code: "ru",
        label: "Русский",
        path: "ru_RU/ru_RU",
        aff_sha256: "38ce7d4af78e211e9bafe4bf7e3d6a2c420591136cb738ec6648f8fdf6524cd7",
        dic_sha256: "f6047416a0204adbecf3a451b874ec8a97ee37e2cbc714466ef04d8dbcc0d6fc",
        size_kb: 3462,
        licence: "BSD-style (Lebedev)",
    },
    Remote {
        code: "uk",
        label: "Українська",
        path: "uk_UA/uk_UA",
        aff_sha256: "2219dd15e9802adebc45722c60943b1472640260491af38dd3e43b07e75585e6",
        dic_sha256: "2e5a9e67be63bdb089b3459addb5d71113319d13768e277bcae20f3cc1ad5a93",
        size_kb: 8918,
        licence: "MPL-1.1",
    },
    Remote {
        code: "tr",
        label: "Türkçe",
        path: "tr_TR/tr_TR",
        aff_sha256: "a04a227b2ee45574000b876a1afa982e3436d49ebb97a7a796ddc5ec0cc8191b",
        dic_sha256: "48e5352c5770956b24f355265aef7649b25bcb4d933c0b254ea8b419c241f4bd",
        size_kb: 37521,
        licence: "MPL-2.0",
    },
    Remote {
        code: "el",
        label: "Ελληνικά",
        path: "el_GR/el_GR",
        aff_sha256: "868dfe13ad7d34251915a745e44d38542831463d56e60732ba3a5bc1dcca582a",
        dic_sha256: "e5b9b9c2cf05bbc59e03fe302b462dae85968f822f4fc219a8ed2879d6943720",
        size_kb: 9904,
        licence: "MPL-1.1 OR GPL-2.0 OR LGPL-2.1",
    },
];

pub fn remote(code: &str) -> Option<&'static Remote> {
    REMOTE.iter().find(|r| r.code == code)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn verify(bytes: &[u8], expected: &str, what: &str) -> Result<(), Error> {
    let got = hex(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref());
    if got == expected {
        Ok(())
    } else {
        Err(Error(format!(
            "The downloaded {what} file doesn't match the expected checksum, so it was discarded."
        )))
    }
}

/// spellbook reads UTF-8 only: recode legacy dictionaries (Polish and Greek
/// ship as ISO-8859-2/-7) and rewrite their `SET` line to match. It is also
/// stricter than Hunspell about numeric flags, so entries whose flag part
/// isn't digits and commas (Danish `"A/S"`, `"c/o"`) are dropped.
fn to_utf8(aff: &[u8], dic: &[u8]) -> Result<(String, String), Error> {
    let (aff, dic) = recode(aff, dic)?;
    if !aff.lines().any(|l| l.trim() == "FLAG num") {
        return Ok((aff, dic));
    }
    let ok = |l: &str| {
        l.split_once('/').is_none_or(|(_, flags)| {
            let flags = flags.split_whitespace().next().unwrap_or("");
            flags.bytes().all(|b| b.is_ascii_digit() || b == b',')
        })
    };
    let dic = dic.lines().filter(|l| ok(l)).collect::<Vec<_>>().join("\n");
    Ok(if uses_flag_zero(&aff) {
        (remap_zero_flag(&aff, true), remap_zero_flag(&dic, false))
    } else {
        (aff, dic)
    })
}

/// Turkish names an affix class `0`, which spellbook rejects.
const ZERO_FLAG: &str = "65000";

fn uses_flag_zero(aff: &str) -> bool {
    aff.lines()
        .any(|l| l.starts_with("SFX 0 ") || l.starts_with("PFX 0 "))
}

fn map_flags(list: &str) -> String {
    list.split(',')
        .map(|f| if f == "0" { ZERO_FLAG } else { f })
        .collect::<Vec<_>>()
        .join(",")
}

/// Renames flag 0 to `ZERO_FLAG` in numeric flag lists: the class and the
/// `/flags` continuation of `SFX`/`PFX` lines (`aff`), or the flags of each
/// word (`.dic`).
fn remap_zero_flag(text: &str, aff: bool) -> String {
    let mut out = String::with_capacity(text.len() + 1024);
    for line in text.lines() {
        if aff {
            let mut t: Vec<String> = line.split_whitespace().map(String::from).collect();
            if matches!(t.first().map(String::as_str), Some("SFX" | "PFX")) && t.len() >= 4 {
                if t[1] == "0" {
                    t[1] = ZERO_FLAG.into();
                }
                if t.len() >= 5
                    && let Some((add, flags)) = t[3].split_once('/')
                {
                    t[3] = format!("{add}/{}", map_flags(flags));
                }
                out.push_str(&t.join(" "));
            } else {
                out.push_str(line);
            }
        } else if let Some((word, rest)) = line.split_once('/') {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            out.push_str(word);
            out.push('/');
            out.push_str(&map_flags(&rest[..end]));
            out.push_str(&rest[end..]);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn recode(aff: &[u8], dic: &[u8]) -> Result<(String, String), Error> {
    let bad = || Error("The downloaded dictionary isn't in a readable encoding.".into());
    let lossy = String::from_utf8_lossy(aff);
    let set = lossy.lines().find(|l| l.starts_with("SET ")).map(str::trim);
    let name = set.map_or("UTF-8", |l| l[4..].trim());
    if name.eq_ignore_ascii_case("UTF-8") {
        let s = |b: &[u8]| String::from_utf8(b.to_vec()).map_err(|_| bad());
        return Ok((s(aff)?, s(dic)?));
    }
    let upper = upper_half(name).ok_or_else(bad)?;
    let aff = decode(aff, &upper).replacen(set.unwrap_or(""), "SET UTF-8", 1);
    Ok((aff, decode(dic, &upper)))
}

/// Bytes 0xA0–0xFF of the two single-byte encodings left in the dictionary list (Polish and
/// Greek); below that both are Latin-1. Unassigned bytes read as U+FFFD, as browsers do.
const ISO_8859_2: &str = "\u{a0}Ą˘Ł¤ĽŚ§¨ŠŞŤŹ\u{ad}ŽŻ°ą˛ł´ľśˇ¸šşťź˝žżŔÁÂĂÄĹĆÇČÉĘËĚÍÎĎĐŃŇÓÔŐÖ×ŘŮÚŰÜÝŢßŕáâăäĺćçčéęëěíîďđńňóôőö÷řůúűüýţ˙";
const ISO_8859_7: &str = "\u{a0}‘’£€₯¦§¨©ͺ«¬\u{ad}\u{fffd}―°±²³΄΅Ά·ΈΉΊ»Ό½ΎΏΐΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡ\u{fffd}ΣΤΥΦΧΨΩΪΫάέήίΰαβγδεζηθικλμνξοπρςστυφχψωϊϋόύώ\u{fffd}";

fn upper_half(label: &str) -> Option<Vec<char>> {
    let norm: String = label
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase();
    let table = match norm.as_str() {
        "ISO88592" | "LATIN2" => ISO_8859_2,
        "ISO88597" | "GREEK" => ISO_8859_7,
        _ => return None,
    };
    Some(table.chars().collect())
}

fn decode(bytes: &[u8], upper: &[char]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0..0xA0 => b as char,
            _ => upper[(b - 0xA0) as usize],
        })
        .collect()
}

async fn fetch(client: &reqwest::Client, url: &str, label: &str) -> Result<Vec<u8>, Error> {
    let offline = |e: reqwest::Error| {
        Error(format!(
            "Couldn't download the {label} dictionary. Check your internet connection and try again. ({e})"
        ))
    };
    let res = client.get(url).send().await.map_err(offline)?;
    if !res.status().is_success() {
        return Err(Error(format!(
            "Couldn't download the {label} dictionary: GitHub answered {}.",
            res.status()
        )));
    }
    Ok(res.bytes().await.map_err(offline)?.to_vec())
}

/// Downloads `code` into `<root>/<code>/index.{aff,dic}`. Files are checked
/// before anything is written, and the folder appears all at once.
pub async fn download(root: &Path, code: &str) -> Result<(), Error> {
    let r = remote(code).ok_or_else(|| Error(format!("No dictionary for {code}.")))?;
    static PROVIDER: std::sync::Once = std::sync::Once::new();
    // Err means another provider is already installed, which is fine.
    PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(900))
        .user_agent(concat!("Lemmata/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error(e.to_string()))?;
    let [aff_url, dic_url] = r.urls();
    let aff = fetch(&client, &aff_url, r.label).await?;
    verify(&aff, r.aff_sha256, r.label)?;
    let dic = fetch(&client, &dic_url, r.label).await?;
    verify(&dic, r.dic_sha256, r.label)?;
    let (aff, dic) = to_utf8(&aff, &dic)?;
    let io = |e: std::io::Error| Error(format!("Couldn't save the {} dictionary: {e}", r.label));
    let tmp = root.join(format!(".{code}.part"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(io)?;
    std::fs::write(tmp.join("index.aff"), aff).map_err(io)?;
    std::fs::write(tmp.join("index.dic"), dic).map_err(io)?;
    let dest = root.join(code);
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&tmp, &dest).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_complete() {
        assert_eq!(REMOTE.len(), 14);
        for r in REMOTE {
            let [a, d] = r.urls();
            assert!(a.ends_with(".aff") && d.ends_with(".dic"), "{}", r.code);
            assert!(
                a.contains(COMMIT) && a.starts_with("https://"),
                "{}",
                r.code
            );
            for h in [r.aff_sha256, r.dic_sha256] {
                assert!(
                    h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()),
                    "{}",
                    r.code
                );
            }
            assert!(r.size_kb > 0 && !r.licence.is_empty(), "{}", r.code);
            assert!(
                crate::LANGUAGES.iter().all(|&l| l != r.code),
                "{} is bundled",
                r.code
            );
        }
        let mut codes: Vec<_> = REMOTE.iter().map(|r| r.code).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), REMOTE.len());
    }

    #[test]
    fn verifies_hashes() {
        // SHA-256 of "abc".
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify(b"abc", abc, "x").is_ok());
        assert!(verify(b"abd", abc, "x").unwrap_err().0.contains("checksum"));
    }

    #[test]
    fn both_legacy_encodings_read_like_encoding_rs() {
        let all: Vec<u8> = (0..=255).collect();
        for (label, enc) in [
            ("ISO8859-2", encoding_rs::ISO_8859_2),
            ("ISO8859-7", encoding_rs::ISO_8859_7),
        ] {
            assert_eq!(
                decode(&all, &upper_half(label).unwrap()),
                enc.decode(&all).0,
                "{label}"
            );
        }
        assert!(upper_half("KOI8-R").is_none());
    }

    #[test]
    fn recodes_legacy_encodings() {
        let (aff, dic) = to_utf8(b"SET ISO8859-2\nTRY \xb1\n", b"1\n\xb1\n").unwrap();
        assert_eq!(aff, "SET UTF-8\nTRY ą\n");
        assert_eq!(dic, "1\ną\n");
        assert!(to_utf8(b"SET UTF-8\n", b"\xff").is_err());
        let (_, dic) =
            to_utf8(b"SET UTF-8\nFLAG num\n", b"3\n\"A/S\"\nau pair/37\nword\n").unwrap();
        assert_eq!(dic, "3\nau pair/37\nword");
        let (aff, dic) = to_utf8(
            b"SET UTF-8\nFLAG num\nSFX 0 N 1\nSFX 0 0 en/0,5 .\n",
            b"1\nabc/0,7 ph:x\n",
        )
        .unwrap();
        assert_eq!(
            aff,
            "SET UTF-8\nFLAG num\nSFX 65000 N 1\nSFX 65000 0 en/65000,5 .\n"
        );
        assert_eq!(dic, "1\nabc/65000,7 ph:x\n");
    }

    /// Needs the network: `cargo test -p lemmata-spell -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn downloads_a_dictionary() {
        let dir = tempfile::tempdir().unwrap();
        download(dir.path(), "pt-PT").await.unwrap();
        let sp = crate::Speller::load(&[dir.path()], "pt-PT").unwrap();
        assert!(sp.check("computador") && !sp.check("computadro"));
    }
}
