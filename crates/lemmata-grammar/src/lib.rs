//! Grammar checking against a LanguageTool HTTP server. Nothing here talks to
//! any host but the URL the caller passes: the app never defaults to a public
//! server, so text leaves the machine only when the user chose an address.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// One finding. `offset` and `length` count UTF-16 code units, as LanguageTool
/// reports them, which is what JavaScript string indices use too.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub offset: usize,
    pub length: usize,
    pub message: String,
    pub short_message: String,
    pub replacements: Vec<String>,
    pub rule_id: String,
    pub category: String,
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    matches: Vec<Raw>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    offset: usize,
    length: usize,
    #[serde(default)]
    message: String,
    #[serde(default)]
    short_message: String,
    #[serde(default)]
    replacements: Vec<Replacement>,
    rule: Rule,
}

#[derive(Deserialize)]
struct Replacement {
    value: String,
}

#[derive(Deserialize)]
struct Rule {
    id: String,
    category: Option<Category>,
}

#[derive(Deserialize)]
struct Category {
    id: String,
}

/// Parses a `/v2/check` response body.
pub fn parse(json: &str) -> Result<Vec<Match>, String> {
    let r: Response =
        serde_json::from_str(json).map_err(|e| format!("Unexpected LanguageTool answer: {e}"))?;
    Ok(r.matches
        .into_iter()
        .map(|m| Match {
            offset: m.offset,
            length: m.length,
            message: m.message,
            short_message: m.short_message,
            replacements: m
                .replacements
                .into_iter()
                .map(|r| r.value)
                .take(5)
                .collect(),
            rule_id: m.rule.id,
            category: m.rule.category.map(|c| c.id).unwrap_or_default(),
        })
        .collect())
}

/// reqwest is built without a TLS crypto provider; install ring (once) so a
/// self-hosted https server works without a second crypto stack.
fn client() -> Result<reqwest::Client, String> {
    static PROVIDER: std::sync::Once = std::sync::Once::new();
    PROVIDER.call_once(|| {
        // Err means another provider is already installed, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())
}

fn endpoint(url: &str, path: &str) -> Result<String, String> {
    let base = url.trim().trim_end_matches('/');
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err("The LanguageTool address must start with http:// or https://.".into());
    }
    Ok(format!("{base}{path}"))
}

fn unreachable(url: &str, e: &reqwest::Error) -> String {
    if e.is_timeout() {
        format!("LanguageTool at {url} took too long to answer.")
    } else {
        format!("No LanguageTool server at {url}. Start one, or change the address in Preferences.")
    }
}

/// Checks `text` (plain prose) in `language` (en-US, de-DE, fr, ...).
pub async fn check(url: &str, language: &str, text: &str) -> Result<Vec<Match>, String> {
    let res = client()?
        .post(endpoint(url, "/v2/check")?)
        // Spelling is the spell checker's job.
        .form(&[
            ("text", text),
            ("language", language),
            ("disabledCategories", "TYPOS"),
        ])
        .send()
        .await
        .map_err(|e| unreachable(url, &e))?;
    let status = res.status();
    let body = res.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        let detail = body.lines().next().unwrap_or("").trim();
        return Err(format!("LanguageTool answered {status}: {detail}"));
    }
    parse(&body)
}

/// Ok when a LanguageTool server answers at `url`.
pub async fn status(url: &str) -> Result<(), String> {
    let res = client()?
        .get(endpoint(url, "/v2/languages")?)
        .send()
        .await
        .map_err(|e| unreachable(url, &e))?;
    if res.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "{url} answered {}; is it a LanguageTool server?",
            res.status()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../tests/sample.json");

    #[test]
    fn parses_matches() {
        let m = parse(SAMPLE).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].offset, 3);
        assert_eq!(m[0].length, 5);
        assert_eq!(m[0].rule_id, "EN_A_VS_AN");
        assert_eq!(m[0].category, "GRAMMAR");
        assert_eq!(m[0].short_message, "Wrong article");
        assert_eq!(m[0].replacements, ["an"]);
        // At most five replacements, no short message is fine.
        assert_eq!(m[1].replacements.len(), 5);
        assert_eq!(m[1].short_message, "");
    }

    #[test]
    fn rejects_garbage_and_bad_urls() {
        assert!(parse("<html>").is_err());
        assert!(endpoint("localhost:8081", "/v2/check").is_err());
        assert_eq!(
            endpoint(" http://localhost:8081/ ", "/v2/check").unwrap(),
            "http://localhost:8081/v2/check"
        );
    }

    #[tokio::test]
    async fn reports_missing_server() {
        // Port 1 is never a LanguageTool server.
        let e = status("http://127.0.0.1:1").await.unwrap_err();
        assert!(
            e.starts_with("No LanguageTool server at http://127.0.0.1:1."),
            "{e}"
        );
    }
}
