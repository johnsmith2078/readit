use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use edge_tts_rust::{Boundary, EdgeTtsClient, SpeakOptions as EdgeSpeakOptions};

use crate::ReaditError;

const DEFAULT_VOICE: &str = "zh-CN-XiaoxiaoNeural";
const DEFAULT_RATE: &str = "+0%";
const DEFAULT_VOLUME: &str = "+0%";
const DEFAULT_PITCH: &str = "+0Hz";

pub async fn synthesize_to_temp_mp3(
    text: &str,
    voice: Option<&str>,
    rate: Option<&str>,
    volume: Option<&str>,
) -> Result<PathBuf, ReaditError> {
    let path = temp_mp3_path()?;
    let options = EdgeSpeakOptions {
        voice: value_or_default(voice, DEFAULT_VOICE).to_owned(),
        rate: value_or_default(rate, DEFAULT_RATE).to_owned(),
        volume: value_or_default(volume, DEFAULT_VOLUME).to_owned(),
        pitch: DEFAULT_PITCH.to_owned(),
        boundary: Boundary::Sentence,
    };

    let client = EdgeTtsClient::builder()
        .connect_timeout(Duration::from_secs(10))
        .receive_timeout(Duration::from_secs(45))
        .ws_warmup(false)
        .build()
        .map_err(|error| ReaditError::EdgeTts(error.to_string()))?;

    client
        .save(text.to_owned(), options, &path, None::<&Path>)
        .await
        .map_err(|error| ReaditError::EdgeTts(error.to_string()))?;

    Ok(path)
}

fn value_or_default<'a>(value: Option<&'a str>, default: &'a str) -> &'a str {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(default)
}

fn temp_mp3_path() -> Result<PathBuf, ReaditError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| ReaditError::EdgeTts(error.to_string()))?
        .as_millis();
    let mut path = std::env::temp_dir();
    path.push(format!("readit-{}-{millis}.mp3", std::process::id()));
    Ok(path)
}
