mod board;
mod slack_data;

use board::show_board;
use bytes::Buf;
use core::fmt;
use futures::SinkExt;
use futures_util::StreamExt;
use image::{
    gif::GifDecoder,
    imageops::{resize, FilterType},
    load_from_memory, AnimationDecoder, ImageFormat,
};
use regex::Regex;
use rpi_led_matrix::LedMatrix;
use serde::Deserialize;
use slack_data::{SlackConfirmation, SlackConnection, SlackEvent, SlackEvents, SlackMessage};
use std::collections::HashMap;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

#[derive(Debug, Deserialize)]
pub struct StandardEmoji {
    pub short_names: Vec<String>,
    pub image: String,
}

#[derive(Debug, Deserialize)]
pub struct EmojiMap {
    pub ok: bool,
    pub emoji: HashMap<String, String>,
}

#[derive(Debug)]
enum CustomError {
    GenericError(String),
}

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let CustomError::GenericError(message) = self;
        write!(f, "{}", message)
    }
}

const WS_TOKEN: &'static str = "xapp-1-A02F4B9F84Q-2940342413297-0886bd0f32d4991f22bb654047e50d340b27bbf2db6b731af513fe572da2a352";
const TOKEN: &'static str =
    "xoxp-4284532945-2041666172486-2924877078885-7883c47d09bc7aa3f514ce5ad63260c1";

const CHANNEL_ID: &'static str = "C02E06BJQ1M";
const EMOJI_RE: &'static str = r":(\S*):";
const STD_EMOJI_PREFIX: &'static str =
    "https://raw.githubusercontent.com/iamcal/emoji-data/master/img-google-64/";

async fn build_standard_emoji_hashmap() -> reqwest::Result<HashMap<String, String>> {
    let mut m = HashMap::new();

    let standard_emojis: Vec<StandardEmoji> =
        reqwest::get("https://raw.githubusercontent.com/iamcal/emoji-data/master/emoji.json")
            .await?
            .json()
            .await?;

    for standard_emoji in standard_emojis {
        for name in standard_emoji.short_names {
            let mut image_url = standard_emoji.image.clone();
            image_url.insert_str(0, STD_EMOJI_PREFIX);
            m.insert(name, image_url);
        }
    }

    Ok(m)
}

async fn build_custom_emoji_hashmap() -> reqwest::Result<HashMap<String, String>> {
    let client = reqwest::Client::new();
    let emoji_map: EmojiMap = client
        .get("https://slack.com/api/emoji.list")
        .bearer_auth(TOKEN)
        .send()
        .await?
        .json()
        .await?;

    Ok(emoji_map.emoji)
}

enum SlackEmoji {
    Success,
    Failure,
    NotAnImage,
}

async fn post_slack_emoji(success: SlackEmoji, timestamp: &str) -> Result<(), CustomError> {
    let name = match success {
        SlackEmoji::Failure => "sad-cowboy",
        SlackEmoji::NotAnImage => "interrobang",
        SlackEmoji::Success => "robot_face",
    };

    let client = reqwest::Client::new();
    client
        .post("https://slack.com/api/reactions.add")
        .bearer_auth(TOKEN)
        .query(&[
            ("channel", CHANNEL_ID),
            ("name", name),
            ("timestamp", timestamp),
        ])
        .send()
        .await
        .map(|_| ())
        .map_err(|e| CustomError::GenericError(e.to_string()))
}

async fn get_connection_url() -> Result<SlackConnection, CustomError> {
    let client = reqwest::Client::new();
    client
        .post("https://slack.com/api/apps.connections.open")
        .bearer_auth(WS_TOKEN)
        .send()
        .await
        .map_err(|e| CustomError::GenericError(e.to_string()))?
        .json()
        .await
        .map_err(|e| CustomError::GenericError(e.to_string()))
}

fn get_image_type_from_url(url: &str) -> ImageFormat {
    let file_format = url.split(".").collect::<Vec<&str>>().pop();

    match file_format {
        Some("gif") => ImageFormat::Gif,
        _ => ImageFormat::Jpeg,
    }
}

async fn process_image(url: &str) -> Result<LedMatrix, CustomError> {
    // https://www.reddit.com/r/rust/comments/g2zeps/how_do_i_get_an_image_from_a_url/
    let bytes = reqwest::get(url)
        .await
        .map_err(|e| CustomError::GenericError(e.to_string()))?
        .bytes()
        .await
        .map_err(|e| CustomError::GenericError(e.to_string()))?;

    match get_image_type_from_url(url) {
        ImageFormat::Gif => {
            let decoder = GifDecoder::new(bytes.reader()).unwrap();
            let frames = decoder.into_frames().collect_frames().unwrap();

            let _resized_frames: Vec<_> = frames
                .iter()
                .map(|frame| resize(frame.buffer(), 64, 64, FilterType::Lanczos3))
                .collect();

            Err(CustomError::GenericError(
                "GIFs are not supported".to_owned(),
            ))
        }
        _ => {
            let img =
                load_from_memory(&bytes).map_err(|e| CustomError::GenericError(e.to_string()))?;

            let buf = resize(&img, 64, 64, FilterType::Lanczos3);

            show_board(buf.enumerate_pixels()).map_err(|e| CustomError::GenericError(e.to_string()))
        }
    }
}

async fn generate_url_from_message<'a, 'b>(
    message: &'a str,
    standard_emojis: &'b HashMap<String, String>,
    custom_emojis: &'b HashMap<String, String>,
) -> Option<&'b String> {
    let emoji_re = Regex::new(EMOJI_RE).unwrap();
    match emoji_re.captures(message) {
        Some(m) => {
            let captured_match = m.get(1).unwrap().as_str().to_owned();

            let standard_emoji_url = standard_emojis.get(&captured_match);
            if standard_emoji_url.is_some() {
                return standard_emoji_url;
            }

            custom_emojis.get(&captured_match)
        }
        None => None,
    }
}

async fn get_messages(
    url: &str,
    standard_emojis: &HashMap<String, String>,
    custom_emojis: &HashMap<String, String>,
) -> Result<(), CustomError> {
    let parsed_url = Url::parse(url).expect("Invalid URL");

    let (ws_stream, _) = connect_async(parsed_url)
        .await
        .map_err(|e| CustomError::GenericError(e.to_string()))?;
    let (mut sink, mut read) = ws_stream.split();
    let mut _channel_ref = None;

    while let Some(message) = read.next().await {
        if let Ok(msg) = message {
            let data = msg.into_data();

            if let Ok(m) = std::str::from_utf8(&data) {
                println!("{}", m);
            } else {
                println!("Unable to parse message into utf8");
            }

            if let Ok(event) = serde_json::from_slice::<SlackMessage>(&data) {
                match event {
                    SlackMessage::Message(m) => {
                        let confirmation = SlackConfirmation {
                            envelope_id: &m.envelope_id,
                        };

                        let message = Message::Text(serde_json::to_string(&confirmation).unwrap());
                        sink.send(message).await.unwrap();

                        if let Ok(event) = serde_json::from_value::<SlackEvent>(m.payload) {
                            match event.event {
                                SlackEvents::Message(message) => {
                                    if message.channel != CHANNEL_ID {
                                        println!("Ignoring message to irrelevant channel");
                                    } else {
                                        if let Some(url) = generate_url_from_message(
                                            &message.text,
                                            standard_emojis,
                                            custom_emojis,
                                        )
                                        .await
                                        {
                                            let result = process_image(url).await;

                                            if let Ok(tx) = result {
                                                _channel_ref = Some(tx);
                                                post_slack_emoji(SlackEmoji::Success, &message.ts)
                                                    .await?;
                                            } else {
                                                post_slack_emoji(SlackEmoji::Failure, &message.ts)
                                                    .await?;
                                            }
                                        } else {
                                            post_slack_emoji(SlackEmoji::NotAnImage, &message.ts)
                                                .await?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    SlackMessage::Disconnect(_) => return Ok(()),
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    loop {
        let data: SlackConnection = get_connection_url().await.unwrap();

        let standard_emojis = build_standard_emoji_hashmap().await.unwrap();
        let custom_emojis = build_custom_emoji_hashmap().await.unwrap();

        match get_messages(&data.url, &standard_emojis, &custom_emojis).await {
            Err(err) => {
                println!("{}", err.to_string());
            }
            _ => {
                println!("Received disconnect; reconnecting...");
            }
        }
    }
}
