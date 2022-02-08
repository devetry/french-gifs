use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize)]
pub struct SlackConnection {
    pub ok: bool,
    pub url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SlackHello {}

#[derive(Debug, Deserialize, Serialize)]
pub struct SlackDisconnect {}

#[derive(Debug, Deserialize, Serialize)]
pub struct SlackEventMessage {
    pub channel: String,
    pub text: String,
    pub ts: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum SlackEvents {
    #[serde(rename = "message")]
    Message(SlackEventMessage),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SlackEvent {
    pub event: SlackEvents,
    #[serde(rename = "type")]
    pub event_type: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SlackEventsApi {
    pub envelope_id: String,
    pub payload: Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum SlackMessage {
    #[serde(rename = "disconnect")]
    Disconnect(SlackDisconnect),
    #[serde(rename = "hello")]
    Hello(SlackHello),
    #[serde(rename = "events_api")]
    Message(SlackEventsApi),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SlackConfirmation<'a> {
    pub envelope_id: &'a str,
}
