use std::time::Duration;

use serde::Deserialize;

use crate::error::Error;
use crate::merge::FetchedEmoji;
use crate::model::pack::Format;

const DEFAULT_BASE: &str = "https://api.telegram.org";
const API_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_API_BODY: u64 = 4 * 1024 * 1024;
const MAX_DOWNLOAD: u64 = 20 * 1024 * 1024;
const MAX_RETRIES: u32 = 5;
const MAX_RETRY_AFTER: u64 = 60;
pub const MAX_STICKERS: usize = 1000;

pub struct BotApi {
    token: String,
    base: String,
    api_agent: ureq::Agent,
    download_agent: ureq::Agent,
}

impl std::fmt::Debug for BotApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BotApi")
            .field("token", &"<BOT_TOKEN>")
            .field("base", &self.base)
            .finish()
    }
}

// Telegram adds fields over time, so these structs deliberately accept unknown ones.
#[derive(Debug, Deserialize)]
pub struct StickerSet {
    pub name: String,
    pub title: String,
    pub sticker_type: String,
    pub stickers: Vec<Sticker>,
}

#[derive(Debug, Deserialize)]
pub struct Sticker {
    pub file_id: String,
    pub file_unique_id: String,
    pub custom_emoji_id: Option<String>,
    pub emoji: Option<String>,
    #[serde(default)]
    pub is_animated: bool,
    #[serde(default)]
    pub is_video: bool,
    pub needs_repainting: Option<bool>,
}

#[derive(Deserialize)]
struct Envelope<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
    parameters: Option<ResponseParameters>,
}

#[derive(Deserialize)]
struct ResponseParameters {
    retry_after: Option<u64>,
}

#[derive(Deserialize)]
struct TelegramFile {
    file_path: Option<String>,
}

enum Outcome<T> {
    Done(T),
    RateLimited(u64),
}

pub fn redact(text: &str, token: &str) -> String {
    if token.is_empty() {
        return text.to_string();
    }
    text.replace(token, "<BOT_TOKEN>")
        .replace(&token.replace(':', "%3A"), "<BOT_TOKEN>")
}

fn is_emoji_id(id: &str) -> bool {
    (1..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

impl StickerSet {
    /// `limit` is the largest accepted sticker count, `None` lifts it.
    pub fn into_fetched(self, limit: Option<usize>) -> Result<Vec<FetchedEmoji>, Error> {
        if self.sticker_type != "custom_emoji" {
            return Err(Error::Runtime(format!(
                "sticker set {} has type {}, only custom_emoji sets are supported",
                self.name, self.sticker_type
            )));
        }
        if let Some(max) = limit
            && self.stickers.len() > max
        {
            return Err(Error::Runtime(format!(
                "sticker set {} has {} stickers, the limit is {max}; pass --no-limit to build it anyway",
                self.name,
                self.stickers.len()
            )));
        }
        let mut out = Vec::with_capacity(self.stickers.len());
        for (index, sticker) in self.stickers.into_iter().enumerate() {
            let position = index + 1;
            let id = match sticker.custom_emoji_id {
                Some(id) if is_emoji_id(&id) => id,
                Some(_) => {
                    return Err(Error::Runtime(format!(
                        "sticker {position} of {}: custom_emoji_id is not 1-20 digits",
                        self.name
                    )));
                }
                None => {
                    return Err(Error::Runtime(format!(
                        "sticker {position} of {}: custom_emoji_id is missing",
                        self.name
                    )));
                }
            };
            let fallback = match sticker.emoji {
                Some(emoji) if !emoji.is_empty() => emoji,
                _ => {
                    return Err(Error::Runtime(format!(
                        "sticker {position} of {}: emoji is missing or empty",
                        self.name
                    )));
                }
            };
            let format = if sticker.is_animated {
                Format::Tgs
            } else if sticker.is_video {
                Format::Webm
            } else {
                Format::Static
            };
            out.push(FetchedEmoji {
                n: position as u32,
                id,
                file_unique_id: sticker.file_unique_id,
                fallback,
                format,
                repaint: sticker.needs_repainting.unwrap_or(false),
                file_id: sticker.file_id,
            });
        }
        Ok(out)
    }
}

fn build_agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        // never follow a redirect: the request URL carries the token
        .max_redirects(0)
        .build()
        .into()
}

impl BotApi {
    pub fn from_env() -> Result<BotApi, Error> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<BotApi, Error> {
        let token = lookup("BOT_TOKEN")
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .ok_or_else(|| Error::Runtime("BOT_TOKEN is not set".into()))?;
        let base = lookup("TGE_BOT_API_BASE")
            .map(|b| b.trim().trim_end_matches('/').to_string())
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE.to_string());
        Ok(BotApi {
            token,
            base,
            api_agent: build_agent(API_TIMEOUT),
            download_agent: build_agent(DOWNLOAD_TIMEOUT),
        })
    }

    pub fn get_sticker_set(&self, name: &str) -> Result<StickerSet, Error> {
        self.call("getStickerSet", &[("name", name)])
    }

    pub fn download(&self, file_id: &str) -> Result<Vec<u8>, Error> {
        let file: TelegramFile = self.call("getFile", &[("file_id", file_id)])?;
        let path = file
            .file_path
            .filter(|p| !p.is_empty() && !p.starts_with('/') && !p.split('/').any(|s| s == ".."))
            .ok_or_else(|| self.fail("getFile returned no usable file_path".into()))?;
        let url = format!("{}/file/bot{}/{path}", self.base, self.token);
        self.retrying(|| {
            let (status, body) = self.send(&self.download_agent, &url, &[], MAX_DOWNLOAD)?;
            if (200..300).contains(&status) {
                return Ok(Outcome::Done(body));
            }
            if status == 429 {
                return Ok(Outcome::RateLimited(retry_after(&body).unwrap_or(1)));
            }
            Err(self.fail(format!("file download failed with HTTP {status}")))
        })
    }

    fn call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        query: &[(&str, &str)],
    ) -> Result<T, Error> {
        let url = format!("{}/bot{}/{method}", self.base, self.token);
        self.retrying(|| {
            let (status, body) = self.send(&self.api_agent, &url, query, MAX_API_BODY)?;
            let envelope: Option<Envelope<T>> = serde_json::from_slice(&body).ok();
            let Some(envelope) = envelope else {
                if status == 429 {
                    return Ok(Outcome::RateLimited(1));
                }
                return Err(self.fail(format!(
                    "{method} returned an unreadable response (HTTP {status})"
                )));
            };
            if envelope.ok && (200..300).contains(&status) {
                return match envelope.result {
                    Some(result) => Ok(Outcome::Done(result)),
                    None => Err(self.fail(format!("{method} returned no result"))),
                };
            }
            if let Some(wait) = envelope.parameters.and_then(|p| p.retry_after) {
                return Ok(Outcome::RateLimited(wait));
            }
            if status == 429 {
                return Ok(Outcome::RateLimited(1));
            }
            let description = envelope.description.unwrap_or_default();
            Err(self.fail(format!("{method} failed (HTTP {status}): {description}")))
        })
    }

    fn send(
        &self,
        agent: &ureq::Agent,
        url: &str,
        query: &[(&str, &str)],
        limit: u64,
    ) -> Result<(u16, Vec<u8>), Error> {
        let mut request = agent.get(url);
        for (key, value) in query {
            request = request.query(key, value);
        }
        let mut response = request
            .call()
            .map_err(|err| self.fail(format!("request failed: {err}")))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            // ureq errors when the body reaches the limit, so allow one extra byte
            .limit(limit + 1)
            .read_to_vec()
            .map_err(|err| self.fail(format!("reading the response failed: {err}")))?;
        Ok((status, body))
    }

    fn retrying<T>(
        &self,
        mut attempt: impl FnMut() -> Result<Outcome<T>, Error>,
    ) -> Result<T, Error> {
        let mut retries = 0;
        loop {
            match attempt()? {
                Outcome::Done(value) => return Ok(value),
                Outcome::RateLimited(wait) => {
                    if wait > MAX_RETRY_AFTER {
                        return Err(self.fail(format!(
                            "Telegram asked to wait {wait} s, which is too long"
                        )));
                    }
                    if retries == MAX_RETRIES {
                        return Err(self.fail(format!(
                            "still rate limited by Telegram after {MAX_RETRIES} retries"
                        )));
                    }
                    retries += 1;
                    eprintln!("rate limited by Telegram, retrying in {wait} s");
                    std::thread::sleep(Duration::from_secs(wait));
                }
            }
        }
    }

    fn fail(&self, message: String) -> Error {
        Error::Runtime(redact(&message, &self.token))
    }
}

fn retry_after(body: &[u8]) -> Option<u64> {
    serde_json::from_slice::<Envelope<serde_json::Value>>(body)
        .ok()?
        .parameters?
        .retry_after
}
