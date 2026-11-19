//! Zotero through the Better BibTeX plugin's local server (127.0.0.1:23119).
//! Nothing leaves the computer.

use crate::Error;
use regex::Regex;
use std::sync::LazyLock;
use std::time::Duration;

const BASE: &str = "http://127.0.0.1:23119/better-bibtex";

fn client(timeout: Duration) -> Result<reqwest::Client, Error> {
    crate::client_builder()
        .timeout(timeout)
        .no_proxy()
        .build()
        .map_err(|e| Error(e.to_string()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Ready,
    /// Zotero is running but Better BibTeX is still starting.
    Starting,
    /// Zotero isn't running, or Better BibTeX isn't installed.
    Unavailable,
}

pub async fn status() -> Status {
    let Ok(c) = client(Duration::from_secs(2)) else {
        return Status::Unavailable;
    };
    match c.get(format!("{BASE}/cayw?probe=true")).send().await {
        Ok(r) => match r.text().await.as_deref().map(str::trim) {
            Ok("ready") => Status::Ready,
            Ok(_) => Status::Starting,
            Err(_) => Status::Unavailable,
        },
        Err(_) => Status::Unavailable,
    }
}

static CITE_KEYS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{([^}]*)\}").unwrap());

/// Opens Zotero's citation picker and waits for the user's choice.
/// Returns the chosen citation keys (empty if they cancelled).
pub async fn pick() -> Result<Vec<String>, Error> {
    let res = client(Duration::from_secs(600))?
        .get(format!("{BASE}/cayw?format=latex&command=cite"))
        .send()
        .await
        .map_err(|_| Error("Zotero isn't running, or Better BibTeX isn't installed.".into()))?;
    let text = res.text().await.map_err(|e| Error(e.to_string()))?;
    Ok(parse_keys(&text))
}

pub fn parse_keys(cite: &str) -> Vec<String> {
    CITE_KEYS
        .captures_iter(cite)
        .flat_map(|c| {
            c[1].split(',')
                .map(|k| k.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|k| !k.is_empty())
        .collect()
}

/// BibLaTeX for the given keys, exported by Better BibTeX. Keys Zotero
/// doesn't know are returned in the second list.
pub async fn export(keys: &[String]) -> Result<(String, Vec<String>), Error> {
    let c = client(Duration::from_secs(30))?;
    let offline = || Error("Zotero isn't running, or Better BibTeX isn't installed.".into());
    // Err(true): Zotero couldn't be reached at all; Err(false): it answered with an error.
    let call = |keys: Vec<String>| {
        let c = c.clone();
        async move {
            let body = serde_json::json!({
                "jsonrpc": "2.0", "method": "item.export", "id": 1,
                "params": [keys, "Better BibLaTeX"]
            });
            let res: serde_json::Value = c
                .post(format!("{BASE}/json-rpc"))
                .json(&body)
                .send()
                .await
                .map_err(|_| true)?
                .json()
                .await
                .map_err(|_| false)?;
            match res.get("result") {
                // Older Better BibTeX versions answer [status, format, text].
                Some(serde_json::Value::Array(a)) => {
                    Ok(a.last().and_then(|v| v.as_str()).unwrap_or("").to_string())
                }
                Some(v) => Ok(v.as_str().unwrap_or("").to_string()),
                None => Err(false),
            }
        }
    };
    // One call for everything; if a key is unknown, fall back to one by one.
    match call(keys.to_vec()).await {
        Ok(all) => return Ok((all, Vec::new())),
        // Unreachable: every key would come back "unknown", and zotero.bib be emptied.
        Err(true) => return Err(offline()),
        Err(false) => {}
    }
    let mut text = String::new();
    let mut missing = Vec::new();
    for k in keys {
        match call(vec![k.clone()]).await {
            Ok(t) => {
                text.push_str(t.trim_end());
                text.push_str("\n\n");
            }
            Err(true) => return Err(offline()),
            Err(false) => missing.push(k.clone()),
        }
    }
    Ok((text, missing))
}

#[cfg(test)]
mod tests {
    /// With Zotero closed the export used to report every key "unknown" and succeed, so the
    /// sync emptied zotero.bib. It fails instead.
    #[tokio::test]
    async fn an_unreachable_zotero_is_an_error_not_an_empty_export() {
        if std::net::TcpStream::connect("127.0.0.1:23119").is_ok() {
            return; // Zotero runs on this computer
        }
        let e = super::export(&["a".to_string(), "b".to_string()])
            .await
            .unwrap_err();
        assert!(e.0.contains("isn't running"), "{}", e.0);
    }

    #[test]
    fn parses_cayw_output() {
        assert_eq!(
            super::parse_keys("\\cite{knuth1984, lamport1994}"),
            ["knuth1984", "lamport1994"]
        );
        assert!(super::parse_keys("").is_empty());
    }
}
