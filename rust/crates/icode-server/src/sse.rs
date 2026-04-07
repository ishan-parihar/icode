use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures::stream::StreamExt;
use runtime::EventBus;
use tokio::sync::mpsc;
pub fn event_bus_to_sse(
    e: &EventBus,
) -> Sse<impl futures::Stream<Item = Result<SseEvent, std::convert::Infallible>>> {
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let _unsub = e.subscribe_all(move |ev| {
        let _ = tx.send(serde_json::to_string(&ev).unwrap_or_default());
    });
    Sse::new(
        tokio_stream::wrappers::UnboundedReceiverStream::new(rx)
            .map(|t| Ok(SseEvent::default().event("event").data(t))),
    )
    .keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    )
}
