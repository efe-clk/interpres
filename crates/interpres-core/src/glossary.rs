//! Glossaries: term pairs for one language pair, applied by engines that support them.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlossaryTerm {
    pub source: String,
    pub target: String,
}

/// The terms for one language pair, with a hash that changes whenever they do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Glossary {
    /// Base language codes, e.g. `TR` and `EN`.
    pub source_lang: String,
    pub target_lang: String,
    pub terms: Vec<GlossaryTerm>,
    pub hash: String,
}

impl Glossary {
    pub fn new(source_lang: &str, target_lang: &str, mut terms: Vec<GlossaryTerm>) -> Self {
        terms.sort_by(|a, b| a.source.cmp(&b.source));
        let source_lang = base_lang(source_lang);
        let target_lang = base_lang(target_lang);
        let mut h = Sha256::new();
        h.update(format!("{source_lang}\t{target_lang}\n"));
        for t in &terms {
            h.update(format!("{}\t{}\n", t.source, t.target));
        }
        Self {
            source_lang,
            target_lang,
            terms,
            hash: hex(&h.finalize()),
        }
    }
}

/// `en-us` → `EN`. Glossaries are per base language.
pub fn base_lang(code: &str) -> String {
    code.split('-').next().unwrap_or(code).to_uppercase()
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(s: &str, t: &str) -> GlossaryTerm {
        GlossaryTerm {
            source: s.into(),
            target: t.into(),
        }
    }

    #[test]
    fn hash_ignores_order_and_tracks_content() {
        let a = Glossary::new("tr", "EN-US", vec![term("a", "1"), term("b", "2")]);
        let b = Glossary::new("TR", "en", vec![term("b", "2"), term("a", "1")]);
        let c = Glossary::new("TR", "EN", vec![term("a", "1"), term("b", "3")]);
        assert_eq!(a.hash, b.hash);
        assert_ne!(a.hash, c.hash);
        assert_eq!(
            (a.source_lang.as_str(), a.target_lang.as_str()),
            ("TR", "EN")
        );
    }
}
