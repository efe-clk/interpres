//! Keeps one DeepL glossary per language pair in sync with the local terms.
//!
//! DeepL v2 glossaries cannot be edited, so a changed term list gets a new glossary
//! and the old one is deleted. Glossaries are found by name: `interpres TR-EN <hash>`.

use interpres_core::glossary::base_lang;
use interpres_core::{EngineError, Glossary};
use serde::{Deserialize, Serialize};

use crate::{DeeplEngine, parse};

#[derive(Deserialize)]
struct List {
    glossaries: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    glossary_id: String,
    name: String,
}

#[derive(Serialize)]
struct Create<'a> {
    name: &'a str,
    source_lang: String,
    target_lang: String,
    entries: String,
    entries_format: &'static str,
}

fn pair_prefix(source_lang: &str, target_lang: &str) -> String {
    format!(
        "interpres {}-{} ",
        base_lang(source_lang),
        base_lang(target_lang)
    )
}

fn prefix(g: &Glossary) -> String {
    pair_prefix(&g.source_lang, &g.target_lang)
}

fn name(g: &Glossary) -> String {
    format!("{}{}", prefix(g), &g.hash[..16])
}

/// Tabs and line breaks would break the TSV format.
fn clean(s: &str) -> String {
    s.replace(['\t', '\r', '\n'], " ").trim().to_string()
}

fn tsv(g: &Glossary) -> String {
    g.terms
        .iter()
        .map(|t| format!("{}\t{}", clean(&t.source), clean(&t.target)))
        .collect::<Vec<_>>()
        .join("\n")
}

impl DeeplEngine {
    /// Deletes the DeepL glossaries Interpres created for a language pair. Used when
    /// the pair has no terms left.
    pub async fn delete_glossaries(
        &self,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<(), EngineError> {
        let prefix = pair_prefix(source_lang, target_lang);
        let list: List = parse(self.send(self.http.get(self.url("/v2/glossaries"))).await?).await?;
        for item in list
            .glossaries
            .iter()
            .filter(|i| i.name.starts_with(&prefix))
        {
            let url = self.url(&format!("/v2/glossaries/{}", item.glossary_id));
            self.send(self.http.delete(url)).await?;
        }
        Ok(())
    }

    /// Returns the id of a DeepL glossary matching `g`, creating it if needed and
    /// deleting outdated ones for the same pair.
    pub async fn ensure_glossary(&self, g: &Glossary) -> Result<String, EngineError> {
        let wanted = name(g);
        let list: List = parse(self.send(self.http.get(self.url("/v2/glossaries"))).await?).await?;

        let mut found = None;
        for item in list
            .glossaries
            .iter()
            .filter(|i| i.name.starts_with(&prefix(g)))
        {
            if item.name == wanted && found.is_none() {
                found = Some(item.glossary_id.clone());
            } else {
                let url = self.url(&format!("/v2/glossaries/{}", item.glossary_id));
                if let Err(e) = self.send(self.http.delete(url)).await {
                    tracing::warn!(error = %e, glossary = %item.name, "could not delete old glossary");
                }
            }
        }
        if let Some(id) = found {
            return Ok(id);
        }

        let body = Create {
            name: &wanted,
            source_lang: g.source_lang.to_lowercase(),
            target_lang: g.target_lang.to_lowercase(),
            entries: tsv(g),
            entries_format: "tsv",
        };
        let created: Item = parse(
            self.send(self.http.post(self.url("/v2/glossaries")).json(&body))
                .await?,
        )
        .await?;
        Ok(created.glossary_id)
    }

    /// The glossary id to send with a request, if it has a glossary and a source language.
    pub(crate) async fn glossary_id(
        &self,
        source: Option<&str>,
        glossary: Option<&Glossary>,
    ) -> Result<Option<String>, EngineError> {
        match (source, glossary) {
            (Some(_), Some(g)) if !g.terms.is_empty() => Ok(Some(self.ensure_glossary(g).await?)),
            _ => Ok(None),
        }
    }
}
