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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use pavis_ingest_api::Artifact;

    struct MockIngest;
    #[async_trait::async_trait]
    impl Ingest for MockIngest {
        type Stream = futures_util::stream::Empty<Result<Artifact, IngestError>>;
        async fn stream(&mut self) -> Result<Self::Stream, IngestError> {
            Ok(futures_util::stream::empty())
        }
    }

    #[tokio::test]
    async fn boxed_ingest_delegates_stream() {
        let mut ingest = boxed_ingest(MockIngest);
        let stream = ingest.stream().await;
        assert!(stream.is_ok());
        assert!(stream.unwrap().next().await.is_none());
    }

    #[tokio::test]
    async fn boxed_ingest_handles_error() {
        struct ErrIngest;
        #[async_trait::async_trait]
        impl Ingest for ErrIngest {
            type Stream = futures_util::stream::Empty<Result<Artifact, IngestError>>;
            async fn stream(&mut self) -> Result<Self::Stream, IngestError> {
                Err(IngestError::Io(anyhow::anyhow!("fail")))
            }
        }

        let mut ingest = boxed_ingest(ErrIngest);
        let res = ingest.stream().await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_ingest_box_explicit() {
        struct Mock;
        #[async_trait::async_trait]
        impl Ingest for Mock {
            type Stream = futures_util::stream::Empty<Result<Artifact, IngestError>>;
            async fn stream(&mut self) -> Result<Self::Stream, IngestError> {
                Ok(futures_util::stream::empty())
            }
        }

        let mut ib = IngestBox(Mock);
        let res = ib.stream().await;
        assert!(res.is_ok());
    }
}
