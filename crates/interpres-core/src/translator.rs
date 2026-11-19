use std::sync::Arc;

use crate::{Engine, EngineError, TranslateRequest, Translation};

/// Entry point used by every front-end: routes a request to the primary engine and
/// falls back to a second engine when the primary fails.
pub struct Translator {
    primary: Arc<dyn Engine>,
    fallback: Option<Arc<dyn Engine>>,
}

impl Translator {
    pub fn new(primary: Arc<dyn Engine>, fallback: Option<Arc<dyn Engine>>) -> Self {
        Self { primary, fallback }
    }

    pub fn primary(&self) -> &Arc<dyn Engine> {
        &self.primary
    }

    pub async fn translate(&self, req: &TranslateRequest) -> Result<Translation, EngineError> {
        match self.primary.translate(req).await {
            Ok(t) => Ok(t),
            Err(e) => match &self.fallback {
                Some(fb) if e.is_retryable_elsewhere() => {
                    tracing::warn!(
                        primary = %self.primary.id(),
                        fallback = %fb.id(),
                        error = %e,
                        "primary engine failed, falling back"
                    );
                    fb.translate(req).await
                }
                _ => Err(e),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Capabilities, EngineId, Language, LanguageKind};
    use async_trait::async_trait;

    struct Fake {
        id: EngineId,
        result: Result<&'static str, EngineError>,
    }

    #[async_trait]
    impl Engine for Fake {
        fn id(&self) -> EngineId {
            self.id
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                streaming: false,
                documents: false,
                glossary: false,
                formality: false,
            }
        }
        async fn translate(&self, _: &TranslateRequest) -> Result<Translation, EngineError> {
            self.result.clone().map(|t| Translation {
                text: t.into(),
                detected_source: None,
                engine: self.id,
            })
        }
        async fn languages(&self, _: LanguageKind) -> Result<Vec<Language>, EngineError> {
            Ok(vec![])
        }
    }

    fn req() -> TranslateRequest {
        TranslateRequest {
            text: "Merhaba".into(),
            source: None,
            target: "EN-US".into(),
            formality: Default::default(),
        }
    }

    fn fake(id: EngineId, result: Result<&'static str, EngineError>) -> Arc<dyn Engine> {
        Arc::new(Fake { id, result })
    }

    #[tokio::test]
    async fn uses_primary_when_it_succeeds() {
        let t = Translator::new(
            fake(EngineId::ClaudeApi, Ok("Hello")),
            Some(fake(EngineId::Deepl, Ok("Hi"))),
        );
        let out = t.translate(&req()).await.unwrap();
        assert_eq!(out.engine, EngineId::ClaudeApi);
    }

    #[tokio::test]
    async fn falls_back_on_failure() {
        let t = Translator::new(
            fake(
                EngineId::ClaudeApi,
                Err(EngineError::Network("down".into())),
            ),
            Some(fake(EngineId::Deepl, Ok("Hi"))),
        );
        let out = t.translate(&req()).await.unwrap();
        assert_eq!(out.engine, EngineId::Deepl);
        assert_eq!(out.text, "Hi");
    }

    #[tokio::test]
    async fn does_not_fall_back_on_invalid_request() {
        let t = Translator::new(
            fake(
                EngineId::ClaudeApi,
                Err(EngineError::InvalidRequest("bad".into())),
            ),
            Some(fake(EngineId::Deepl, Ok("Hi"))),
        );
        assert!(matches!(
            t.translate(&req()).await,
            Err(EngineError::InvalidRequest(_))
        ));
    }

    #[tokio::test]
    async fn returns_error_without_fallback() {
        let t = Translator::new(fake(EngineId::Deepl, Err(EngineError::QuotaExceeded)), None);
        assert_eq!(t.translate(&req()).await, Err(EngineError::QuotaExceeded));
    }

    #[tokio::test]
    async fn default_stream_yields_single_piece() {
        use futures::StreamExt;
        let e = fake(EngineId::Deepl, Ok("Hello"));
        let pieces: Vec<_> = e.translate_stream(&req()).await.unwrap().collect().await;
        assert_eq!(pieces, vec![Ok("Hello".to_string())]);
    }
}
