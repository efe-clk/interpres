use std::sync::Arc;

use crate::store::{Store, cache_key};
use crate::{Engine, EngineError, TranslateRequest, Translation};

/// Entry point used by every front-end: applies the glossary, answers from the cache
/// when it can, routes the request to the engine and records the result in the
/// history.
pub struct Translator {
    primary: Arc<dyn Engine>,
    store: Option<Arc<Store>>,
}

impl Translator {
    pub fn new(primary: Arc<dyn Engine>) -> Self {
        Self {
            primary,
            store: None,
        }
    }

    /// Enables glossary, cache and history.
    pub fn with_store(mut self, store: Arc<Store>) -> Self {
        self.store = Some(store);
        self
    }

    pub fn primary(&self) -> &Arc<dyn Engine> {
        &self.primary
    }

    pub async fn translate(&self, req: &TranslateRequest) -> Result<Translation, EngineError> {
        let Some(store) = &self.store else {
            return self.primary.translate(req).await;
        };

        let mut req = req.clone();
        // Glossaries need an explicit source language.
        if req.glossary.is_none()
            && let Some(src) = &req.source
        {
            req.glossary = store
                .glossary(src, &req.target)
                .map_err(|e| EngineError::Service(e.to_string()))?;
        }

        let key = cache_key(self.primary.id(), &req);
        let cached = store.cache_get(&key).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "cache read failed");
            None
        });
        let t = match cached {
            Some((text, detected_source)) => Translation {
                text,
                detected_source,
                engine: self.primary.id(),
            },
            None => {
                let t = self.primary.translate(&req).await?;
                if let Err(e) = store.cache_put(&key, &t) {
                    tracing::warn!(error = %e, "cache write failed");
                }
                t
            }
        };
        if let Err(e) = store.record(&req, &t) {
            tracing::warn!(error = %e, "history write failed");
        }
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use crate::{Capabilities, EngineId, Language, LanguageKind};
    use async_trait::async_trait;

    struct Fake {
        id: EngineId,
        result: Result<&'static str, EngineError>,
        calls: std::sync::atomic::AtomicUsize,
        last: std::sync::Mutex<Option<TranslateRequest>>,
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
        async fn translate(&self, req: &TranslateRequest) -> Result<Translation, EngineError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            *self.last.lock().unwrap() = Some(req.clone());
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
            glossary: None,
        }
    }

    fn fake(id: EngineId, result: Result<&'static str, EngineError>) -> Arc<dyn Engine> {
        counting(id, result)
    }

    fn counting(id: EngineId, result: Result<&'static str, EngineError>) -> Arc<Fake> {
        Arc::new(Fake {
            id,
            result,
            calls: Default::default(),
            last: Default::default(),
        })
    }

    fn calls(f: &Fake) -> usize {
        f.calls.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[tokio::test]
    async fn second_request_comes_from_cache_and_both_are_recorded() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        let engine = counting(EngineId::Deepl, Ok("Hello"));
        let t = Translator::new(engine.clone()).with_store(store.clone());
        assert_eq!(t.translate(&req()).await.unwrap().text, "Hello");
        assert_eq!(t.translate(&req()).await.unwrap().text, "Hello");
        assert_eq!(calls(&engine), 1);
        assert_eq!(store.history("", 10).unwrap().len(), 2);

        store.cache_clear().unwrap();
        t.translate(&req()).await.unwrap();
        assert_eq!(calls(&engine), 2);
    }

    #[tokio::test]
    async fn applies_glossary_only_with_source_language() {
        let store = Arc::new(Store::open_in_memory().unwrap());
        store
            .upsert_term(None, "TR", "EN", "Merhaba", "Howdy")
            .unwrap();
        let engine = counting(EngineId::Deepl, Ok("Howdy"));
        let t = Translator::new(engine.clone()).with_store(store.clone());

        t.translate(&req()).await.unwrap();
        assert!(
            engine
                .last
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .glossary
                .is_none()
        );

        let mut r = req();
        r.source = Some("TR".into());
        t.translate(&r).await.unwrap();
        let g = engine
            .last
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .glossary
            .clone()
            .unwrap();
        assert_eq!(g.terms[0].target, "Howdy");

        // Changing the glossary invalidates the cache.
        t.translate(&r).await.unwrap();
        assert_eq!(calls(&engine), 2);
        store
            .upsert_term(None, "TR", "EN", "dünya", "world")
            .unwrap();
        t.translate(&r).await.unwrap();
        assert_eq!(calls(&engine), 3);
    }

    #[tokio::test]
    async fn returns_engine_error() {
        let t = Translator::new(fake(EngineId::Deepl, Err(EngineError::QuotaExceeded)));
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
