//! Local SQLite database: glossary terms, translation cache, translation memory and
//! history. One file in the OS data directory.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::glossary::{Glossary, GlossaryTerm, base_lang, hex};
use crate::{EngineId, Formality, TranslateRequest, Translation};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not determine the data directory")]
    NoDataDir,
    #[error("could not create {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("invalid CSV: {0}")]
    Csv(#[from] csv::Error),
    #[error("{0}")]
    Invalid(String),
}

type Result<T> = std::result::Result<T, StoreError>;

/// Schema versions, applied in order. `PRAGMA user_version` records how many ran.
const MIGRATIONS: &[&str] = &["
    CREATE TABLE glossary_terms (
        id          INTEGER PRIMARY KEY,
        source_lang TEXT NOT NULL,
        target_lang TEXT NOT NULL,
        source      TEXT NOT NULL,
        target      TEXT NOT NULL,
        UNIQUE (source_lang, target_lang, source)
    );
    CREATE TABLE cache (
        key             TEXT PRIMARY KEY,
        text            TEXT NOT NULL,
        detected_source TEXT,
        created_at      INTEGER NOT NULL
    );
    CREATE TABLE tm (
        id          INTEGER PRIMARY KEY,
        source_lang TEXT NOT NULL,
        target_lang TEXT NOT NULL,
        source      TEXT NOT NULL,
        target      TEXT NOT NULL,
        engine      TEXT NOT NULL,
        created_at  INTEGER NOT NULL,
        UNIQUE (source_lang, target_lang, source, engine)
    );
    CREATE TABLE history (
        id              INTEGER PRIMARY KEY,
        source_text     TEXT NOT NULL,
        target_text     TEXT NOT NULL,
        source_lang     TEXT,
        target_lang     TEXT NOT NULL,
        detected_source TEXT,
        engine          TEXT NOT NULL,
        created_at      INTEGER NOT NULL
    );
    CREATE INDEX history_created ON history (created_at DESC);
"];

/// A language pair that has glossary terms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlossaryPair {
    pub source_lang: String,
    pub target_lang: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermRow {
    pub id: i64,
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub source_text: String,
    pub target_text: String,
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub detected_source: Option<String>,
    pub engine: EngineId,
    /// Unix seconds.
    pub created_at: i64,
}

pub struct Store {
    conn: Mutex<Connection>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn engine_from_str(s: &str) -> EngineId {
    match s {
        "claude-api" => EngineId::ClaudeApi,
        "claude-code" => EngineId::ClaudeCode,
        _ => EngineId::Deepl,
    }
}

fn formality_str(f: Formality) -> &'static str {
    match f {
        Formality::Default => "default",
        Formality::More => "more",
        Formality::Less => "less",
        Formality::PreferMore => "prefer_more",
        Formality::PreferLess => "prefer_less",
    }
}

/// Cache key: engine, language pair, formality, glossary hash and text.
pub fn cache_key(engine: EngineId, req: &TranslateRequest) -> String {
    let mut h = Sha256::new();
    for part in [
        engine.as_str(),
        req.source.as_deref().unwrap_or("auto"),
        &req.target,
        formality_str(req.formality),
        req.glossary.as_ref().map_or("", |g| g.hash.as_str()),
        &req.text,
    ] {
        h.update(part.as_bytes());
        h.update([0]);
    }
    hex(&h.finalize())
}

impl Store {
    /// `<data dir>/interpres/interpres.db`.
    pub fn default_path() -> Result<PathBuf> {
        directories::ProjectDirs::from("", "", "interpres")
            .map(|d| d.data_dir().join("interpres.db"))
            .ok_or(StoreError::NoDataDir)
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| StoreError::Io {
                path: dir.into(),
                source,
            })?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", i as i64 + 1)?;
            tx.commit()?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---------- Glossary ----------

    pub fn glossary_pairs(&self) -> Result<Vec<GlossaryPair>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT source_lang, target_lang, COUNT(*) FROM glossary_terms
             GROUP BY source_lang, target_lang ORDER BY source_lang, target_lang",
        )?;
        let rows = st.query_map([], |r| {
            Ok(GlossaryPair {
                source_lang: r.get(0)?,
                target_lang: r.get(1)?,
                count: r.get::<_, i64>(2)? as usize,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn glossary_terms(&self, source_lang: &str, target_lang: &str) -> Result<Vec<TermRow>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT id, source, target FROM glossary_terms
             WHERE source_lang = ?1 AND target_lang = ?2 ORDER BY source COLLATE NOCASE",
        )?;
        let rows = st.query_map(
            params![base_lang(source_lang), base_lang(target_lang)],
            |r| {
                Ok(TermRow {
                    id: r.get(0)?,
                    source: r.get(1)?,
                    target: r.get(2)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Inserts a term, or updates it when `id` is given. A term with the same source
    /// text in the same pair is replaced.
    pub fn upsert_term(
        &self,
        id: Option<i64>,
        source_lang: &str,
        target_lang: &str,
        source: &str,
        target: &str,
    ) -> Result<()> {
        let (source, target) = (source.trim(), target.trim());
        if source.is_empty() || target.is_empty() {
            return Err(StoreError::Invalid(
                "term and translation must not be empty".into(),
            ));
        }
        let conn = self.conn();
        if let Some(id) = id {
            conn.execute("DELETE FROM glossary_terms WHERE id = ?1", [id])?;
        }
        conn.execute(
            "INSERT INTO glossary_terms (source_lang, target_lang, source, target)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (source_lang, target_lang, source) DO UPDATE SET target = excluded.target",
            params![
                base_lang(source_lang),
                base_lang(target_lang),
                source,
                target
            ],
        )?;
        Ok(())
    }

    pub fn delete_term(&self, id: i64) -> Result<()> {
        self.conn()
            .execute("DELETE FROM glossary_terms WHERE id = ?1", [id])?;
        Ok(())
    }

    /// The glossary for a pair, or `None` when it has no terms.
    pub fn glossary(&self, source_lang: &str, target_lang: &str) -> Result<Option<Glossary>> {
        let terms: Vec<GlossaryTerm> = self
            .glossary_terms(source_lang, target_lang)?
            .into_iter()
            .map(|t| GlossaryTerm {
                source: t.source,
                target: t.target,
            })
            .collect();
        Ok((!terms.is_empty()).then(|| Glossary::new(source_lang, target_lang, terms)))
    }

    /// Reads `source,target` rows (no header). Returns how many terms were stored.
    pub fn import_csv(
        &self,
        source_lang: &str,
        target_lang: &str,
        reader: impl Read,
    ) -> Result<usize> {
        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(reader);
        let mut n = 0;
        for rec in rdr.records() {
            let rec = rec?;
            let (Some(s), Some(t)) = (rec.get(0), rec.get(1)) else {
                continue;
            };
            if s.trim().is_empty() || t.trim().is_empty() {
                continue;
            }
            self.upsert_term(None, source_lang, target_lang, s, t)?;
            n += 1;
        }
        Ok(n)
    }

    /// Writes `source,target` rows (no header).
    pub fn export_csv(
        &self,
        source_lang: &str,
        target_lang: &str,
        writer: impl Write,
    ) -> Result<usize> {
        let terms = self.glossary_terms(source_lang, target_lang)?;
        let mut w = csv::Writer::from_writer(writer);
        for t in &terms {
            w.write_record([&t.source, &t.target])?;
        }
        w.flush().map_err(csv::Error::from)?;
        Ok(terms.len())
    }

    // ---------- Cache ----------

    pub fn cache_get(&self, key: &str) -> Result<Option<(String, Option<String>)>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT text, detected_source FROM cache WHERE key = ?1",
                [key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn cache_put(&self, key: &str, t: &Translation) -> Result<()> {
        self.conn().execute(
            "INSERT OR REPLACE INTO cache (key, text, detected_source, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![key, t.text, t.detected_source, now()],
        )?;
        Ok(())
    }

    pub fn cache_clear(&self) -> Result<()> {
        self.conn().execute("DELETE FROM cache", [])?;
        Ok(())
    }

    // ---------- Translation memory and history ----------

    /// Records a finished translation in the translation memory and the history.
    pub fn record(&self, req: &TranslateRequest, t: &Translation) -> Result<()> {
        let conn = self.conn();
        let source_lang = req.source.as_deref().or(t.detected_source.as_deref());
        if let Some(src) = source_lang {
            conn.execute(
                "INSERT OR REPLACE INTO tm (source_lang, target_lang, source, target, engine, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![src.to_uppercase(), req.target, req.text, t.text, t.engine.as_str(), now()],
            )?;
        }
        conn.execute(
            "INSERT INTO history (source_text, target_text, source_lang, target_lang,
                                  detected_source, engine, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                req.text,
                t.text,
                req.source,
                req.target,
                t.detected_source,
                t.engine.as_str(),
                now()
            ],
        )?;
        Ok(())
    }

    /// Newest first. `query` matches source or translated text, case-insensitively.
    pub fn history(&self, query: &str, limit: usize) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn();
        let mut st = conn.prepare(
            "SELECT id, source_text, target_text, source_lang, target_lang, detected_source,
                    engine, created_at
             FROM history
             WHERE ?1 = '' OR source_text LIKE ?2 ESCAPE '\\' OR target_text LIKE ?2 ESCAPE '\\'
             ORDER BY created_at DESC, id DESC LIMIT ?3",
        )?;
        let q = query.trim();
        let pattern = format!(
            "%{}%",
            q.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let rows = st.query_map(params![q, pattern, limit as i64], |r| {
            Ok(HistoryEntry {
                id: r.get(0)?,
                source_text: r.get(1)?,
                target_text: r.get(2)?,
                source_lang: r.get(3)?,
                target_lang: r.get(4)?,
                detected_source: r.get(5)?,
                engine: engine_from_str(&r.get::<_, String>(6)?),
                created_at: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(text: &str) -> TranslateRequest {
        TranslateRequest {
            text: text.into(),
            source: Some("TR".into()),
            target: "EN-US".into(),
            formality: Formality::Default,
            glossary: None,
        }
    }

    fn tr(text: &str) -> Translation {
        Translation {
            text: text.into(),
            detected_source: Some("TR".into()),
            engine: EngineId::Deepl,
        }
    }

    #[test]
    fn migrations_are_idempotent() {
        let path = std::env::temp_dir().join(format!("interpres-store-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Store::open(&path)
            .unwrap()
            .upsert_term(None, "TR", "EN", "a", "b")
            .unwrap();
        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.glossary_terms("TR", "EN").unwrap().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn glossary_crud_and_base_codes() {
        let s = Store::open_in_memory().unwrap();
        s.upsert_term(None, "tr", "EN-US", "sunucu", "server")
            .unwrap();
        s.upsert_term(None, "TR", "EN-GB", "istemci", "client")
            .unwrap();
        // Same source text replaces the translation.
        s.upsert_term(None, "TR", "EN", "sunucu", "host").unwrap();
        let terms = s.glossary_terms("TR", "EN").unwrap();
        assert_eq!(terms.len(), 2);
        assert_eq!(terms[1].target, "host");

        s.upsert_term(Some(terms[0].id), "TR", "EN", "istemci", "customer")
            .unwrap();
        assert_eq!(s.glossary_terms("TR", "EN").unwrap()[0].target, "customer");

        s.delete_term(terms[1].id).unwrap();
        assert_eq!(
            s.glossary_pairs().unwrap(),
            vec![GlossaryPair {
                source_lang: "TR".into(),
                target_lang: "EN".into(),
                count: 1
            }]
        );
        assert!(s.glossary("TR", "DE").unwrap().is_none());
        assert_eq!(s.glossary("TR", "EN-US").unwrap().unwrap().terms.len(), 1);
        assert!(s.upsert_term(None, "TR", "EN", " ", "x").is_err());
    }

    #[test]
    fn csv_roundtrip_with_quotes() {
        let s = Store::open_in_memory().unwrap();
        let csv = "sunucu,server\n\"a, b\",\"c \"\"d\"\"\"\n\nyalnız\n";
        assert_eq!(s.import_csv("TR", "EN", csv.as_bytes()).unwrap(), 2);
        let mut out = Vec::new();
        s.export_csv("TR", "EN", &mut out).unwrap();
        let s2 = Store::open_in_memory().unwrap();
        assert_eq!(s2.import_csv("TR", "EN", out.as_slice()).unwrap(), 2);
        assert_eq!(s.glossary_terms("TR", "EN").unwrap(), {
            let mut v = s2.glossary_terms("TR", "EN").unwrap();
            v.iter_mut()
                .zip(s.glossary_terms("TR", "EN").unwrap())
                .for_each(|(a, b)| a.id = b.id);
            v
        });
        assert!(
            s.glossary_terms("TR", "EN")
                .unwrap()
                .iter()
                .any(|t| t.target == "c \"d\"")
        );
    }

    #[test]
    fn cache_key_depends_on_every_input() {
        let base = req("Merhaba");
        let k = cache_key(EngineId::Deepl, &base);
        assert_eq!(k, cache_key(EngineId::Deepl, &base));
        assert_ne!(k, cache_key(EngineId::ClaudeApi, &base));
        let mut r = base.clone();
        r.formality = Formality::More;
        assert_ne!(k, cache_key(EngineId::Deepl, &r));
        let mut r = base.clone();
        r.target = "DE".into();
        assert_ne!(k, cache_key(EngineId::Deepl, &r));
        let mut r = base.clone();
        r.glossary = Some(Glossary::new("TR", "EN", vec![]));
        assert_ne!(k, cache_key(EngineId::Deepl, &r));
    }

    #[test]
    fn cache_put_get_clear() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.cache_get("k").unwrap().is_none());
        s.cache_put("k", &tr("Hello")).unwrap();
        assert_eq!(
            s.cache_get("k").unwrap(),
            Some(("Hello".into(), Some("TR".into())))
        );
        s.cache_clear().unwrap();
        assert!(s.cache_get("k").unwrap().is_none());
    }

    #[test]
    fn history_search_newest_first() {
        let s = Store::open_in_memory().unwrap();
        s.record(&req("Birinci"), &tr("First")).unwrap();
        s.record(&req("İkinci %50"), &tr("Second")).unwrap();
        let all = s.history("", 10).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].source_text, "İkinci %50");
        assert_eq!(s.history("first", 10).unwrap().len(), 1);
        assert_eq!(s.history("%", 10).unwrap().len(), 1);
        assert_eq!(s.history("yok", 10).unwrap().len(), 0);
        assert_eq!(s.history("", 1).unwrap().len(), 1);
    }
}
