use std::io::Error;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::Duration;
use quinn::{RecvStream, SendStream};

use serde::{Deserialize, Deserializer};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub struct QuicStream {
    pub(crate) bi: (SendStream, RecvStream),
}

impl AsyncRead for QuicStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().bi.1).poll_read(cx, buf)
    }
}

impl AsyncWrite for QuicStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        Pin::new(&mut self.get_mut().bi.0).poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.get_mut().bi.0).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.get_mut().bi.0).poll_shutdown(cx)
    }
}

pub(crate) fn deserialize_cert<'de, D>(d: D) -> Result<Vec<rustls::Certificate>, D::Error> where D: Deserializer<'de> {
    let path = PathBuf::deserialize(d)?;
    let cert = std::fs::read(&path).expect("failed to read certificate");
    Ok(if path.extension().map_or(false, |x| x == "der") {
        vec![rustls::Certificate(cert)]
    } else {
        rustls_pemfile::certs(&mut &*cert)
            .expect("invalid PEM-encoded certificate")
            .into_iter()
            .map(rustls::Certificate)
            .collect()
    })
}

pub(crate) fn deserialize_key<'de, D>(d: D) -> Result<rustls::PrivateKey, D::Error> where D: Deserializer<'de> {
    let path = PathBuf::deserialize(d)?;
    let key = std::fs::read(&path).expect("failed to read private key");
    Ok(if path.extension().map_or(false, |x| x == "der") {
        rustls::PrivateKey(key)
    } else {
        let pkcs8 = rustls_pemfile::pkcs8_private_keys(&mut &*key)
            .expect("malformed PKCS #8 private key");
        match pkcs8.into_iter().next() {
            Some(x) => rustls::PrivateKey(x),
            None => {
                let rsa = rustls_pemfile::rsa_private_keys(&mut &*key)
                    .expect("malformed PKCS #1 private key");
                match rsa.into_iter().next() {
                    Some(x) => rustls::PrivateKey(x),
                    None => {
                        panic!("no private keys found");
                    }
                }
            }
        }
    })
}

pub(crate) fn transport_config() -> quinn::TransportConfig {
    let mut transport_config = quinn::TransportConfig::default();
    transport_config
        .congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));
    transport_config.max_idle_timeout(Some(Duration::from_secs(5).try_into().unwrap()));
    transport_config.keep_alive_interval(Some(Duration::from_secs(3)));

    transport_config
}
