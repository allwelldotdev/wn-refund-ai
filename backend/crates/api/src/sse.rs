//! Server-sent events for `POST /api/conversations/{id}/messages`. The pipeline
//! task writes events into a channel; the response streams them. A client that
//! disconnects only stops receiving: the task still runs to the end.

use std::convert::Infallible;
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use db::conversations::RequestSummary;
use db::messages::Message;
use domain::types::AssistantKind;
use futures_util::Stream;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

const KEEP_ALIVE: Duration = Duration::from_secs(15);
/// Words per `reply_token` event.
const WORDS_PER_TOKEN: usize = 5;

#[derive(Clone, Debug)]
pub enum SseEvent {
    MessageSaved {
        message_id: Uuid,
        seq: i32,
        duplicate: bool,
    },
    ReplyStart {
        kind: AssistantKind,
    },
    ReplyToken {
        text: String,
    },
    ReplyDone {
        message_id: Uuid,
        seq: i32,
        body: String,
    },
    RequestUpdated(RequestSummary),
    Error {
        code: &'static str,
        message: String,
    },
    Done,
}

impl SseEvent {
    fn into_parts(self) -> (&'static str, Value) {
        match self {
            SseEvent::MessageSaved {
                message_id,
                seq,
                duplicate,
            } => (
                "message_saved",
                json!({ "message_id": message_id, "seq": seq, "duplicate": duplicate }),
            ),
            SseEvent::ReplyStart { kind } => ("reply_start", json!({ "kind": kind })),
            SseEvent::ReplyToken { text } => ("reply_token", json!({ "text": text })),
            SseEvent::ReplyDone {
                message_id,
                seq,
                body,
            } => (
                "reply_done",
                json!({ "message_id": message_id, "seq": seq, "body": body }),
            ),
            SseEvent::RequestUpdated(r) => ("request_updated", json!(r)),
            SseEvent::Error { code, message } => {
                ("error", json!({ "code": code, "message": message }))
            }
            SseEvent::Done => ("done", json!({})),
        }
    }

    fn into_event(self) -> Event {
        let (name, data) = self.into_parts();
        Event::default().event(name).data(data.to_string())
    }
}

pub fn stream(rx: mpsc::Receiver<SseEvent>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let events = ReceiverStream::new(rx).map(|e| Ok(e.into_event()));
    Sse::new(events).keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
}

/// Send side of the stream. Send errors mean the client left; they are ignored
/// because the result is already persisted and visible on the next GET.
#[derive(Clone)]
pub struct Emitter(pub mpsc::Sender<SseEvent>);

impl Emitter {
    pub async fn send(&self, event: SseEvent) {
        let _ = self.0.send(event).await;
    }

    /// `reply_start`, the body in word chunks, then `reply_done`. The body was
    /// validated before it was stored, so the chunks are the final text.
    pub async fn reply(&self, message: &Message) {
        let kind = message.assistant_kind.unwrap_or(AssistantKind::Verdict);
        self.send(SseEvent::ReplyStart { kind }).await;
        for text in word_chunks(&message.body) {
            self.send(SseEvent::ReplyToken { text }).await;
        }
        self.send(SseEvent::ReplyDone {
            message_id: message.id,
            seq: message.seq,
            body: message.body.clone(),
        })
        .await;
    }

    pub async fn error(&self, code: &'static str, message: impl Into<String>) {
        self.send(SseEvent::Error {
            code,
            message: message.into(),
        })
        .await;
    }
}

/// Splits after whitespace so the chunks concatenate back to `body` exactly.
fn word_chunks(body: &str) -> Vec<String> {
    let words: Vec<&str> = body.split_inclusive(char::is_whitespace).collect();
    words
        .chunks(WORDS_PER_TOKEN)
        .map(|chunk| chunk.concat())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_rebuild_the_body() {
        let body = "Good news: your refund of $89.99 for Wireless headphones (order ORD-1001) has been approved.";
        let chunks = word_chunks(body);
        assert_eq!(chunks.concat(), body);
        assert_eq!(chunks[0], "Good news: your refund of ");
        assert!(word_chunks("").is_empty());
    }
}
