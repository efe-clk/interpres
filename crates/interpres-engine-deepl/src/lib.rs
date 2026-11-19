//! DeepL engine: text and document translation, languages and usage.
//!
//! Keys ending in `:fx` use the Free endpoint, all others the Pro endpoint.

mod document;

use std::time::Duration;

use async_trait::async_trait;
use interpres_core::{
    Capabilities, Engine, EngineError, EngineId, Formality, Language, LanguageKind,
    TranslateRequest, Translation,
};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};

pub const FREE_URL: &str = "https://api-free.deepl.com";
pub const PRO_URL: &str = "https://api.deepl.com";

/// Picks the API endpoint for a key.
pub fn base_url_for_key(key: &str) -> &'static str {
    if key.ends_with(":fx") {
        FREE_URL
    } else {
        PRO_URL
    }
}

/// Character usage for the current billing period.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Usage {
    pub character_count: u64,
    pub character_limit: u64,
}

pub struct DeeplEngine {
    http: Client,
    base_url: String,
    key: String,
    /// Shortest wait between document status checks.
    poll_interval: Duration,
}

impl DeeplEngine {
    pub fn new(key: impl Into<String>) -> Self {
        let key = key.into();
        let url = base_url_for_key(&key);
        Self::with_base_url(key, url)
    }

    /// Uses an explicit endpoint (tests, proxies).
    pub fn with_base_url(key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            key: key.into(),
            poll_interval: Duration::from_secs(1),
        }
    }

    /// Sets the shortest wait between document status checks (tests use zero).
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn usage(&self) -> Result<Usage, EngineError> {
        let resp = self.send(self.http.get(self.url("/v2/usage"))).await?;
        parse(resp).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    async fn send(&self, rb: RequestBuilder) -> Result<Response, EngineError> {
        let resp = rb
            .header("Authorization", format!("DeepL-Auth-Key {}", self.key))
            .send()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))?;
        check_status(resp).await
    }
}

async fn check_status(resp: Response) -> Result<Response, EngineError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let message = resp
        .json::<ErrorBody>()
        .await
        .ok()
        .and_then(|b| b.message)
        .unwrap_or_else(|| status.to_string());
    Err(match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => EngineError::InvalidKey,
        StatusCode::TOO_MANY_REQUESTS => EngineError::RateLimited,
        // DeepL-specific: 456 Quota Exceeded.
        s if s.as_u16() == 456 => EngineError::QuotaExceeded,
        s if s.is_client_error() => EngineError::InvalidRequest(message),
        _ => EngineError::Service(message),
    })
}

async fn parse<T: for<'de> Deserialize<'de>>(resp: Response) -> Result<T, EngineError> {
    resp.json()
        .await
        .map_err(|e| EngineError::Service(format!("unexpected response: {e}")))
}

#[derive(Deserialize)]
struct ErrorBody {
    message: Option<String>,
}

#[derive(Serialize)]
struct TranslateBody<'a> {
    text: [&'a str; 1],
    target_lang: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_lang: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    formality: Option<&'static str>,
}

#[derive(Deserialize)]
struct TranslateResponse {
    translations: Vec<TranslationItem>,
}

#[derive(Deserialize)]
struct TranslationItem {
    text: String,
    detected_source_language: Option<String>,
}

#[derive(Deserialize)]
struct LanguageItem {
    language: String,
    name: String,
    #[serde(default)]
    supports_formality: bool,
}

fn formality_param(f: Formality) -> Option<&'static str> {
    match f {
        Formality::Default => None,
        Formality::More => Some("more"),
        Formality::Less => Some("less"),
        Formality::PreferMore => Some("prefer_more"),
        Formality::PreferLess => Some("prefer_less"),
    }
}

#[async_trait]
impl Engine for DeeplEngine {
    fn id(&self) -> EngineId {
        EngineId::Deepl
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            streaming: false,
            documents: true,
            glossary: true,
            formality: true,
        }
    }

    async fn translate(&self, req: &TranslateRequest) -> Result<Translation, EngineError> {
        let body = TranslateBody {
            text: [&req.text],
            target_lang: &req.target,
            source_lang: req.source.as_deref(),
            formality: formality_param(req.formality),
        };
        let resp = self
            .send(self.http.post(self.url("/v2/translate")).json(&body))
            .await?;
        let item = parse::<TranslateResponse>(resp)
            .await?
            .translations
            .into_iter()
            .next()
            .ok_or_else(|| EngineError::Service("empty translation list".into()))?;
        Ok(Translation {
            text: item.text,
            detected_source: item.detected_source_language,
            engine: EngineId::Deepl,
        })
    }

    async fn languages(&self, kind: LanguageKind) -> Result<Vec<Language>, EngineError> {
        let kind = match kind {
            LanguageKind::Source => "source",
            LanguageKind::Target => "target",
        };
        let rb = self
            .http
            .get(self.url("/v2/languages"))
            .query(&[("type", kind)]);
        let items: Vec<LanguageItem> = parse(self.send(rb).await?).await?;
        Ok(items
            .into_iter()
            .map(|l| Language {
                code: l.language,
                name: l.name,
                supports_formality: l.supports_formality,
            })
            .collect())
    }
}
