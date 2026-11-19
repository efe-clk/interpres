use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};

use crate::{
    Capabilities, EngineError, EngineId, Language, LanguageKind, TranslateRequest, Translation,
};

/// A stream of translated text pieces, in order. Concatenated they form the full text.
pub type TextStream = BoxStream<'static, Result<String, EngineError>>;

/// Common interface of every translation engine.
#[async_trait]
pub trait Engine: Send + Sync {
    fn id(&self) -> EngineId;

    fn capabilities(&self) -> Capabilities;

    async fn translate(&self, req: &TranslateRequest) -> Result<Translation, EngineError>;

    /// Streams the translation. Engines without native streaming yield one piece.
    async fn translate_stream(&self, req: &TranslateRequest) -> Result<TextStream, EngineError> {
        let t = self.translate(req).await?;
        Ok(stream::once(async move { Ok(t.text) }).boxed())
    }

    async fn languages(&self, kind: LanguageKind) -> Result<Vec<Language>, EngineError>;
}
