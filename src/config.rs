use std::fs::read;
use std::net::SocketAddr;
use std::path::PathBuf;

use compact_str::CompactString;
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
    pub route: FnvHashMap<CompactString, ServerRoute>,
    pub token: CompactString,
    pub max_concurrent_bidi_streams: Option<u32>,

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
    pub remote: CompactString,
    pub route: FnvHashMap<CompactString, ClientRoute>,
    pub token: CompactString,
    pub retry_interval: Option<time_unit::TimeUnit>,
    pub max_retry: Option<usize>,
    pub max_concurrent_bidi_streams: Option<u32>,

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

mod time_unit {
    use std::fmt;
    use std::fmt::{Debug, Formatter};
    use std::str::FromStr;
    use std::time::Duration;

    use serde::de::Visitor;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Debug, Clone, Copy)]
    pub struct TimeUnit(Duration);

    impl TimeUnit {
        pub fn duration(&self) -> &Duration {
            &self.0
        }
    }

    impl fmt::Display for TimeUnit {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    impl FromStr for TimeUnit {
        type Err = parse_duration::parse::Error;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            Ok(TimeUnit(parse_duration::parse(s)?))
        }
    }

    impl Serialize for TimeUnit {
        fn serialize<S>(
            &self,
            serializer: S,
        ) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
        where
            S: Serializer,
        {
            serializer.serialize_str(self.to_string().as_ref())
        }
    }

    impl<'de> Deserialize<'de> for TimeUnit {
        fn deserialize<D>(deserializer: D) -> Result<TimeUnit, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_str(TimeVisitor)
        }
    }

    struct TimeVisitor;

    impl<'de> Visitor<'de> for TimeVisitor {
        type Value = TimeUnit;

        fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
            formatter.write_str("expect a time type")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            TimeUnit::from_str(v).map_err(E::custom)
        }
    }
}
