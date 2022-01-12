use std::fs::read;
use std::net::SocketAddr;
use std::path::PathBuf;

use compact_str::CompactStr;
use fnv::FnvHashMap;

pub fn configuration(config_file: PathBuf) -> anyhow::Result<Config> {
    Ok(toml::from_slice(&read(config_file)?)?)
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Config {
    pub server: Option<ServerConfig>,
    pub client: Option<ClientConfig>,
    #[serde(default = "default_log_level")]
    pub log_level: log::LevelFilter,
}

fn default_log_level() -> log::LevelFilter {
    log::LevelFilter::Info
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub route: FnvHashMap<CompactStr, ServerRoute>,
    pub token: CompactStr,

    #[serde(deserialize_with = "crate::quic::deserialize_cert")]
    pub cert: Vec<rustls::Certificate>,
    #[serde(deserialize_with = "crate::quic::deserialize_key")]
    pub private_key: rustls::PrivateKey,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct ServerRoute {
    pub bind: SocketAddr,
    #[serde(rename = "type")]
    pub _type: RouteType,
    pub udp_buffer: Option<usize>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct ClientConfig {
    pub remote: CompactStr,
    pub route: FnvHashMap<CompactStr, ClientRoute>,
    pub token: CompactStr,

    #[serde(deserialize_with = "crate::quic::deserialize_cert")]
    pub cert: Vec<rustls::Certificate>,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct ClientRoute {
    pub to: SocketAddr,
    #[serde(rename = "type")]
    pub _type: RouteType,
    pub udp_buffer: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, bincode::Encode, bincode::Decode)]
pub enum RouteType {
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "udp")]
    Udp,
}
