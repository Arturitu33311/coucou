// Glass player: shared types. Later tasks (Spotify, YT Music, mpv, commands) use these.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

pub mod history;
#[cfg(target_os = "linux")]
pub mod mpv;
pub mod spotify_api;
pub mod spotify_auth;
#[cfg(target_os = "linux")]
pub mod ytmusic;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Spotify,
    Ytmusic,
}

/// `Context` is a DJ or a list.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Track,
    Artist,
    Album,
    Context,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub source: Source,
    pub kind: Kind,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub art_url: String,
}
