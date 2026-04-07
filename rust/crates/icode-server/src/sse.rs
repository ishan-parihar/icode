use std::convert::Infallible;
use std::time::Duration;

use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures::stream::{Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;

/// Convert an `EventBus` broadcast receiver into an SSE stream.
pub fn event_bus_to_sse(
    event_bus: &runtime::EventBus,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let receiver = event_bus.subscribe();
    let broadcast_stream = BroadcastStream::new(receiver);

    let stream = broadcast_stream.filter_map(|result| async move {
        match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).ok()?;
                Some(Ok(SseEvent::default()
                    .event(event.event_type_name())
                    .data(json)))
            }
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

/// Create an SSE stream from a channel of text deltas.
pub fn text_stream_to_sse(
    rx: tokio::sync::mpsc::Receiver<String>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .map(|text| Ok(SseEvent::default().event("content").data(text)));

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}
