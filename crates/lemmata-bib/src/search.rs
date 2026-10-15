//! Searching for references online by title, author or keywords (Crossref).
//! Only runs when the user asks for it.

use crate::{Error, client_builder, short_authors};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Hit {
    pub title: String,
    /// "Schulman et al.", or empty.
    pub authors: String,
    pub year: String,
    /// Journal, proceedings or book title.
    pub venue: String,
    pub doi: String,
}

#[derive(Deserialize)]
struct Response {
    message: Message,
}
#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    items: Vec<Item>,
}
#[derive(Deserialize)]
struct Item {
    #[serde(rename = "DOI", default)]
    doi: String,
    #[serde(default)]
    title: Vec<String>,
    #[serde(default)]
    author: Vec<Author>,
    #[serde(default)]
    issued: Option<Issued>,
    #[serde(rename = "container-title", default)]
    venue: Vec<String>,
}
#[derive(Deserialize)]
struct Author {
    given: Option<String>,
    family: Option<String>,
    /// Organisations have only a name.
    name: Option<String>,
}
#[derive(Deserialize)]
struct Issued {
    #[serde(rename = "date-parts", default)]
    parts: Vec<Vec<Option<i32>>>,
}

/// The hits in a Crossref `/works` response. Entries without a DOI are dropped.
pub fn parse_response(json: &str) -> Result<Vec<Hit>, Error> {
    let r: Response = serde_json::from_str(json)
        .map_err(|e| Error(format!("Crossref sent something unexpected: {e}")))?;
    Ok(r.message
        .items
        .into_iter()
        .filter(|i| !i.doi.is_empty())
        .map(|i| {
            let names: Vec<String> = i
                .author
                .iter()
                .filter_map(|a| match (&a.family, &a.given, &a.name) {
                    (Some(f), Some(g), _) => Some(format!("{f}, {g}")),
                    (Some(f), None, _) => Some(f.clone()),
                    // The trailing comma makes short_authors keep the whole name.
                    (None, _, Some(n)) => Some(format!("{n},")),
                    _ => None,
                })
                .collect();
            Hit {
                title: i.title.first().map(|t| crate::clean(t)).unwrap_or_default(),
                authors: short_authors(&names.join(" and ")),
                year: i
                    .issued
                    .and_then(|d| d.parts.first().and_then(|p| p.first().copied().flatten()))
                    .map(|y| y.to_string())
                    .unwrap_or_default(),
                venue: i.venue.first().cloned().unwrap_or_default(),
                doi: i.doi,
            }
        })
        .collect())
}

/// Asks Crossref for the ten best matches. The query leaves the computer here.
pub async fn search_online(query: &str) -> Result<Vec<Hit>, Error> {
    let query = query.trim();
    if query.is_empty() {
        return Err(Error(
            "Type a title, an author or some keywords first.".into(),
        ));
    }
    let url = reqwest::Url::parse_with_params(
        "https://api.crossref.org/works",
        [
            ("query.bibliographic", query),
            ("rows", "10"),
            ("select", "DOI,title,author,issued,container-title"),
        ],
    )
    .map_err(|e| Error(e.to_string()))?;
    let client = client_builder()
        .timeout(Duration::from_secs(8))
        .user_agent("Lemmata (https://github.com/NilsBriggen/lemmata)")
        .build()
        .map_err(|e| Error(e.to_string()))?;
    let res = client.get(url).send().await.map_err(|e| {
        Error(if e.is_timeout() {
            "Crossref took too long to answer. Try again in a moment.".into()
        } else if e.is_connect() {
            "Couldn't reach Crossref. Are you offline?".into()
        } else {
            format!("The search failed: {e}")
        })
    })?;
    if !res.status().is_success() {
        return Err(Error(format!("Crossref answered {}.", res.status())));
    }
    let body = res.text().await.map_err(|e| {
        Error(if e.is_timeout() {
            "Crossref took too long to answer. Try again in a moment.".into()
        } else {
            e.to_string()
        })
    })?;
    parse_response(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_stored_crossref_response() {
        let hits = parse_response(include_str!("../testdata/crossref_search.json")).unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(
            hits[0],
            Hit {
                title: "Proximal Policy Optimization Algorithms".into(),
                authors: "Schulman et al.".into(),
                year: "2017".into(),
                venue: "arXiv".into(),
                doi: "10.48550/arxiv.1707.06347".into(),
            }
        );
        // Organisation author, no title, no usable date: still a hit.
        assert_eq!(hits[1].authors, "The Lemmata Consortium");
        assert_eq!(hits[1].year, "");
        assert_eq!(hits[1].title, "");
        assert_eq!(hits[2].doi, "10.1000/bare");
        assert!(parse_response("<html>").is_err());
        assert!(
            parse_response(r#"{"message":{"items":[{"title":["x"]}]}}"#)
                .unwrap()
                .is_empty()
        );
    }

    /// Hits Crossref; run with `cargo test -p lemmata-bib -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn searches_crossref() {
        let hits = search_online("proximal policy optimization schulman")
            .await
            .unwrap();
        assert!(!hits.is_empty());
    }
}
