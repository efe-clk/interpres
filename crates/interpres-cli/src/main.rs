use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use interpres_core::documents::{DocProgress, unique_output_path};
use interpres_core::{
    DocumentEngine, DocumentRequest, EngineError, EngineId, Formality, Settings, Store,
    TranslateRequest, Translator, secrets,
};
use interpres_engine_deepl::DeeplEngine;

/// Interpres command-line translator.
#[derive(Parser)]
#[command(name = "interpres", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Translate text given as arguments or on stdin.
    Translate {
        /// Target language, e.g. EN-US, DE, TR. Defaults to the target in settings.
        #[arg(short, long, value_parser = lang_code)]
        to: Option<String>,
        /// Source language. Auto-detected when omitted.
        #[arg(short, long, value_parser = lang_code)]
        from: Option<String>,
        #[arg(long, value_enum, default_value_t = FormalityArg::Default)]
        formality: FormalityArg,
        /// Text to translate. Read from stdin when omitted.
        text: Vec<String>,
    },
    /// Translate documents, keeping their format. Output goes next to the input
    /// as `name.<LANG>.ext` unless --out is given.
    Doc {
        /// Target language, e.g. EN-US, DE, TR. Defaults to the target in settings.
        #[arg(short, long, value_parser = lang_code)]
        to: Option<String>,
        /// Source language. Auto-detected when omitted.
        #[arg(short, long, value_parser = lang_code)]
        from: Option<String>,
        #[arg(long, value_enum, default_value_t = FormalityArg::Default)]
        formality: FormalityArg,
        /// Output file (only with a single input).
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Manage glossary terms. A glossary applies when the source language is given.
    Glossary {
        #[command(subcommand)]
        action: GlossaryAction,
    },
    /// List past translations, newest first.
    History {
        /// Only entries containing this text.
        query: Option<String>,
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
    },
    /// Manage the translation cache.
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Manage API keys in the OS credential store.
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// Show DeepL character usage for the current period.
    Usage,
}

/// Language pair of a glossary, e.g. `--from tr --to en`.
#[derive(clap::Args)]
struct Pair {
    #[arg(short, long, value_parser = lang_code)]
    from: String,
    #[arg(short, long, value_parser = lang_code)]
    to: String,
}

#[derive(Subcommand)]
enum GlossaryAction {
    /// List language pairs that have terms.
    List,
    /// Show the terms of a pair.
    Show {
        #[command(flatten)]
        pair: Pair,
    },
    /// Add or replace a term.
    Add {
        #[command(flatten)]
        pair: Pair,
        source: String,
        target: String,
    },
    /// Remove a term by its source text.
    Remove {
        #[command(flatten)]
        pair: Pair,
        source: String,
    },
    /// Import `source,target` rows from a CSV file.
    Import {
        #[command(flatten)]
        pair: Pair,
        file: PathBuf,
    },
    /// Export terms as CSV to a file, or to stdout.
    Export {
        #[command(flatten)]
        pair: Pair,
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// Delete all cached translations.
    Clear,
}

#[derive(Subcommand)]
enum KeyAction {
    /// Store a key. The key is read from stdin.
    Set { engine: KeyEngine },
    /// Delete a stored key.
    Delete { engine: KeyEngine },
}

#[derive(Clone, Copy, ValueEnum)]
enum KeyEngine {
    Deepl,
}

impl From<KeyEngine> for EngineId {
    fn from(k: KeyEngine) -> Self {
        match k {
            KeyEngine::Deepl => EngineId::Deepl,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum FormalityArg {
    Default,
    More,
    Less,
    PreferMore,
    PreferLess,
}

impl From<FormalityArg> for Formality {
    fn from(f: FormalityArg) -> Self {
        match f {
            FormalityArg::Default => Formality::Default,
            FormalityArg::More => Formality::More,
            FormalityArg::Less => Formality::Less,
            FormalityArg::PreferMore => Formality::PreferMore,
            FormalityArg::PreferLess => Formality::PreferLess,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    match Cli::parse().command {
        Command::Translate {
            to,
            from,
            formality,
            text,
        } => translate(to, from, formality.into(), text).await,
        Command::Doc {
            to,
            from,
            formality,
            out,
            files,
        } => doc(to, from, formality.into(), out, files).await,
        Command::Key {
            action: KeyAction::Set { engine },
        } => {
            let key = read_key()?;
            secrets::set_key(engine.into(), &key)?;
            eprintln!("Key saved.");
            Ok(())
        }
        Command::Key {
            action: KeyAction::Delete { engine },
        } => {
            secrets::delete_key(engine.into())?;
            eprintln!("Key deleted.");
            Ok(())
        }
        Command::Glossary { action } => glossary(action),
        Command::History { query, limit } => history(query, limit),
        Command::Cache {
            action: CacheAction::Clear,
        } => {
            store()?.cache_clear()?;
            eprintln!("Cache cleared.");
            Ok(())
        }
        Command::Usage => {
            let u = deepl()?.usage().await.map_err(explain)?;
            println!("{} / {} characters", u.character_count, u.character_limit);
            Ok(())
        }
    }
}

async fn translate(
    to: Option<String>,
    from: Option<String>,
    formality: Formality,
    text: Vec<String>,
) -> Result<()> {
    let settings = Settings::load(&Settings::default_path()?)?;
    let target = to
        .or(settings.target_lang)
        .ok_or_else(|| anyhow!("no target language; pass --to, e.g. --to EN-US"))?;
    let text = if text.is_empty() {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .context("could not read stdin")?;
        s
    } else {
        text.join(" ")
    };
    if text.trim().is_empty() {
        bail!("nothing to translate");
    }

    let translator = Translator::new(Arc::new(deepl()?), None).with_store(Arc::new(store()?));
    let req = TranslateRequest {
        text,
        source: from.or(settings.source_lang),
        target,
        formality,
        glossary: None,
    };
    let t = translator.translate(&req).await.map_err(explain)?;
    println!("{}", t.text.trim_end());
    Ok(())
}

async fn doc(
    to: Option<String>,
    from: Option<String>,
    formality: Formality,
    out: Option<PathBuf>,
    files: Vec<PathBuf>,
) -> Result<()> {
    if out.is_some() && files.len() > 1 {
        bail!("--out works with a single input file");
    }
    let settings = Settings::load(&Settings::default_path()?)?;
    let target = to
        .or(settings.target_lang)
        .ok_or_else(|| anyhow!("no target language; pass --to, e.g. --to EN-US"))?;
    let source = from.or(settings.source_lang);
    let req = DocumentRequest {
        glossary: match &source {
            Some(src) => store()?.glossary(src, &target)?,
            None => None,
        },
        source,
        target,
        formality,
    };
    let engine = deepl()?;
    let cancel = tokio_util::sync::CancellationToken::new();

    for input in files {
        let ext = input
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !engine.supports(&ext) {
            bail!("unsupported file type: {}", input.display());
        }
        let output = out
            .clone()
            .unwrap_or_else(|| unique_output_path(&input, &req.target));
        let name = input.display().to_string();
        let progress = |p: DocProgress| {
            let state = match p {
                DocProgress::Uploading => "uploading".to_string(),
                DocProgress::Queued => "queued".to_string(),
                DocProgress::Translating {
                    seconds_remaining: Some(s),
                } => format!("translating, ~{s}s left"),
                DocProgress::Translating { .. } => "translating".to_string(),
                DocProgress::Downloading => "downloading".to_string(),
            };
            eprintln!("{name}: {state}");
        };
        engine
            .translate_document(&input, &output, &req, &progress, &cancel)
            .await
            .map_err(explain)?;
        println!("{}", output.display());
    }
    Ok(())
}

/// DeepL codes are upper case (`en-us` → `EN-US`).
fn lang_code(s: &str) -> Result<String, String> {
    Ok(s.to_uppercase())
}

fn store() -> Result<Store> {
    Ok(Store::open(&Store::default_path()?)?)
}

fn glossary(action: GlossaryAction) -> Result<()> {
    let store = store()?;
    match action {
        GlossaryAction::List => {
            for p in store.glossary_pairs()? {
                println!(
                    "{} -> {}  ({} terms)",
                    p.source_lang, p.target_lang, p.count
                );
            }
        }
        GlossaryAction::Show { pair } => {
            for t in store.glossary_terms(&pair.from, &pair.to)? {
                println!("{}\t{}", t.source, t.target);
            }
        }
        GlossaryAction::Add {
            pair,
            source,
            target,
        } => {
            store.upsert_term(None, &pair.from, &pair.to, &source, &target)?;
        }
        GlossaryAction::Remove { pair, source } => {
            let term = store
                .glossary_terms(&pair.from, &pair.to)?
                .into_iter()
                .find(|t| t.source == source)
                .ok_or_else(|| anyhow!("no such term: {source}"))?;
            store.delete_term(term.id)?;
        }
        GlossaryAction::Import { pair, file } => {
            let f = std::fs::File::open(&file)
                .with_context(|| format!("could not open {}", file.display()))?;
            let n = store.import_csv(&pair.from, &pair.to, f)?;
            eprintln!("{n} terms imported.");
        }
        GlossaryAction::Export { pair, file } => {
            let n = match file {
                Some(path) => {
                    let f = std::fs::File::create(&path)
                        .with_context(|| format!("could not create {}", path.display()))?;
                    store.export_csv(&pair.from, &pair.to, f)?
                }
                None => store.export_csv(&pair.from, &pair.to, std::io::stdout())?,
            };
            eprintln!("{n} terms exported.");
        }
    }
    Ok(())
}

fn history(query: Option<String>, limit: usize) -> Result<()> {
    for h in store()?.history(query.as_deref().unwrap_or(""), limit)? {
        let src = h
            .source_lang
            .or(h.detected_source)
            .unwrap_or_else(|| "?".into());
        println!("{} -> {} [{}]", src, h.target_lang, h.engine);
        println!("  {}", h.source_text.replace('\n', " "));
        println!("  {}", h.target_text.replace('\n', " "));
    }
    Ok(())
}

fn deepl() -> Result<DeeplEngine> {
    let key = secrets::get_key(EngineId::Deepl)?
        .ok_or_else(|| anyhow!("no DeepL key; run: interpres key set deepl"))?;
    Ok(DeeplEngine::new(key))
}

fn read_key() -> Result<String> {
    if std::io::stdin().is_terminal() {
        eprint!("Key: ");
    }
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let key = line.trim();
    if key.is_empty() {
        bail!("empty key");
    }
    Ok(key.to_string())
}

/// Adds a hint to engine errors that the user can act on.
fn explain(e: EngineError) -> anyhow::Error {
    let hint = match e {
        EngineError::MissingKey | EngineError::InvalidKey => {
            "check the key: interpres key set deepl"
        }
        EngineError::QuotaExceeded => "the monthly character limit is reached: interpres usage",
        EngineError::RateLimited => "wait a moment and retry",
        EngineError::Network(_) => "check the internet connection",
        EngineError::InvalidRequest(_) | EngineError::Service(_) | EngineError::Cancelled => "",
    };
    if hint.is_empty() {
        anyhow!(e)
    } else {
        anyhow!("{e} ({hint})")
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
