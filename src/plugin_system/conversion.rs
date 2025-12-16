use super::types::ActivationStreamItem;
use crate::plexus::{PlexusContext, Provenance, PlexusStreamItem};
use futures::{Stream, StreamExt};
use jsonrpsee::{PendingSubscriptionSink, SubscriptionMessage};

pub type SubscriptionResult = Result<(), jsonrpsee::core::StringError>;

/// Trait that enforces Stream<T> can be converted to SubscriptionResult
pub trait IntoSubscription: Send + 'static {
    type Item: ActivationStreamItem;

    /// Convert this stream into a jsonrpsee subscription
    ///
    /// The plexus hash is automatically retrieved from the global PlexusContext.
    async fn into_subscription(
        self,
        pending: PendingSubscriptionSink,
        provenance: Provenance,
    ) -> SubscriptionResult;
}

/// Blanket implementation for any Stream<Item = T> where T: ActivationStreamItem
impl<S, T> IntoSubscription for S
where
    S: Stream<Item = T> + Send + Unpin + 'static,
    T: ActivationStreamItem,
{
    type Item = T;

    async fn into_subscription(
        self,
        pending: PendingSubscriptionSink,
        provenance: Provenance,
    ) -> SubscriptionResult {
        let sink = pending.accept().await?;
        let plexus_hash = PlexusContext::hash();

        tokio::spawn(async move {
            let mut stream = Box::pin(self);
            while let Some(item) = stream.next().await {
                let body_item = item.into_plexus_item(provenance.clone(), &plexus_hash);

                // Convert to SubscriptionMessage and send
                let msg = match SubscriptionMessage::from_json(&body_item) {
                    Ok(msg) => msg,
                    Err(_) => break, // Serialization error, abort stream
                };

                if let Err(_) = sink.send(msg).await {
                    break; // Client disconnected
                }
            }

            // Send Done event when stream completes
            let done = PlexusStreamItem::done(plexus_hash, provenance);
            if let Ok(msg) = SubscriptionMessage::from_json(&done) {
                let _ = sink.send(msg).await;
            }
        });

        Ok(())
    }
}
