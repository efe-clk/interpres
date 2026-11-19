use std::io::{IsTerminal, Read};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use interpres_core::{
    EngineError, EngineId, Formality, Settings, TranslateRequest, Translator, secrets,
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
        #[arg(short, long)]
        to: Option<String>,
        /// Source language. Auto-detected when omitted.
        #[arg(short, long)]
        from: Option<String>,
        #[arg(long, value_enum, default_value_t = FormalityArg::Default)]
        formality: FormalityArg,
        /// Text to translate. Read from stdin when omitted.
        text: Vec<String>,
    },
    /// Manage API keys in the OS credential store.
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// Show DeepL character usage for the current period.
    Usage,
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

    let translator = Translator::new(Arc::new(deepl()?), None);
    let req = TranslateRequest {
        text,
        source: from.or(settings.source_lang),
        target,
        formality,
    };
    let t = translator.translate(&req).await.map_err(explain)?;
    println!("{}", t.text.trim_end());
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
        EngineError::InvalidRequest(_) | EngineError::Service(_) => "",
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
