use futures_util::Stream;
use pavis_ingest_api::{Artifact, Ingest, IngestError};
use std::pin::Pin;

pub type IngestStream = Pin<Box<dyn Stream<Item = Result<Artifact, IngestError>> + Send + 'static>>;
pub type BoxedIngest = Box<dyn Ingest<Stream = IngestStream> + Send>;

struct IngestBox<T>(T);

#[async_trait::async_trait]
impl<T> Ingest for IngestBox<T>
where
    T: Ingest + Send,
    T::Stream: Send + Unpin + 'static,
{
    type Stream = IngestStream;

    async fn stream(&mut self) -> Result<Self::Stream, IngestError> {
        let stream = self.0.stream().await?;
        Ok(Box::pin(stream))
    }
}

pub fn boxed_ingest<T>(ingest: T) -> BoxedIngest
where
    T: Ingest + Send + 'static,
    T::Stream: Send + Unpin + 'static,
{
    Box::new(IngestBox(ingest))
}
