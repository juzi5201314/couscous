use std::sync::Arc;

use anyhow::Context;
use bytes::{Buf, BytesMut};
use integer_encoding::VarIntAsyncReader;
use quinn::Endpoint;
use rustls::RootCertStore;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpStream, UdpSocket};

use crate::config::{ClientConfig, RouteType};
use crate::proto::{
    read_proto, write_proto, Auth, RegisterRoute, RegisterRouteError, RegisterRouteRes,
    StreamStart, VarIntWriter, CODE_AUTH_SUCCESS,
};
use crate::quic::{transport_config, QuicStream};

pub async fn run(config: Arc<ClientConfig>, retry_num: &mut usize) -> anyhow::Result<()> {
    let mut roots = RootCertStore::empty();
    config.cert.iter().try_for_each(|c| roots.add(c))?;

    let ep = Endpoint::client("[::]:0".parse().unwrap())?;
    let mut new_conn = ep
        .connect_with(
            {
                let mut c = quinn::ClientConfig::with_root_certificates(roots);
                c.transport = Arc::new(transport_config());
                Arc::get_mut(&mut c.transport)
                    .unwrap()
                    .max_concurrent_bidi_streams(
                        config.max_concurrent_bidi_streams.unwrap_or(100).into(),
                    );
                c
            },
            tokio::net::lookup_host(&*config.remote)
                .await?
                .next()
                .ok_or_else(|| anyhow::anyhow!("couldn't resolve to an address"))?,
            config.remote.split_once(':').unwrap().0,
        )?
        .await?;
    log::info!("connecting {}", &config.remote);
    *retry_num = 0;

    let mut handshake_stream = new_conn.connection.open_bi().await?;

    // authorization
    write_proto::<_, 32>(
        &mut handshake_stream.0,
        Auth {
            token: config.token.as_str().to_owned(),
        },
    )
    .await?;

    assert_eq!(
        handshake_stream.1.read_u8().await?,
        CODE_AUTH_SUCCESS,
        "authentication failed"
    );

    let register_route = config
        .route
        .iter()
        .map(|(name, route)| RegisterRoute {
            name: name.as_str().to_owned(),
            _type: route._type,
        })
        .collect();

    write_proto::<Vec<RegisterRoute>, 1024>(&mut handshake_stream.0, register_route).await?;

    let res = read_proto::<RegisterRouteRes, 32>(&mut handshake_stream.1).await?;

    match res {
        RegisterRouteRes::Ok => {}
        RegisterRouteRes::Err(RegisterRouteError::Repeated(route)) => {
            anyhow::bail!("route `{}` has been registered on the server", route.name)
        }
        RegisterRouteRes::Err(RegisterRouteError::RouteNotFound(route)) => {
            anyhow::bail!("route: `{}` does not exist on the server", route.name)
        }
        RegisterRouteRes::Err(RegisterRouteError::Other(err, route)) => {
            anyhow::bail!(
                "error: `{}` occurred while registering route `{}`",
                err,
                route.name
            )
        }
    }

    log::info!("handshake finish");

    while let Some(stream) = new_conn.bi_streams.next().await {
        let (mut send_stream, mut recv_stream) = stream?;
        let route_name = read_proto::<StreamStart, 32>(&mut recv_stream)
            .await?
            .route_name;
        let route = config
            .route
            .get(&*route_name)
            .ok_or_else(|| anyhow::anyhow!("Unexpected request route received: {}", &route_name))?;
        let to = route.to;

        match route._type {
            RouteType::Tcp => {
                tokio::spawn(async move {
                    let mut tcp_stream = tokio::io::BufStream::new(
                        TcpStream::connect(to)
                            .await
                            .with_context(|| {
                                format!(
                                    "failed to connect to {}. (route `{}` tcp)",
                                    to, &route_name
                                )
                            })
                            .unwrap(),
                    );
                    log::info!("tcp route connect on `{}`", to);
                    let mut quic_stream = tokio::io::BufStream::new(QuicStream {
                        bi: (send_stream, recv_stream),
                    });
                    tokio::io::copy_bidirectional(&mut tcp_stream, &mut quic_stream)
                        .await
                        .ok();
                    log::info!("tcp route `{}` close", &route_name);
                });
            }
            RouteType::Udp => {
                let udp_buffer_size = route.udp_buffer.unwrap_or(2048);
                let remote_address = new_conn.connection.remote_address();
                tokio::spawn(async move {
                    let socket = Arc::new(UdpSocket::bind("[::]:0").await.unwrap());
                    socket
                        .connect(to)
                        .await
                        .with_context(|| {
                            format!("failed to connect to {}. (route `{}` udp)", to, &route_name)
                        })
                        .unwrap();

                    let socket_cloned = Arc::clone(&socket);
                    let route_name_cloned = route_name.clone();
                    tokio::spawn(async move {
                        let mut buf = BytesMut::with_capacity(udp_buffer_size);
                        let mut buf_reader = tokio::io::BufReader::new(recv_stream);
                        loop {
                            if let anyhow::Result::<_>::Err(_) = try {
                                let len = buf_reader.read_varint_async().await?;
                                buf.resize(len, 0);
                                buf_reader.read_exact(&mut buf).await?;
                                socket_cloned.send(&buf.copy_to_bytes(len)).await?;
                            } {
                                log::info!(
                                    "udp data stream `{}` disconnect. (route: `{}` udp)",
                                    remote_address,
                                    route_name_cloned
                                );
                                break;
                            }
                        }
                    });

                    let mut buf = BytesMut::with_capacity(udp_buffer_size);
                    buf.resize(udp_buffer_size, 0);

                    while let Ok((len, _addr)) = socket.recv_from(&mut buf).await {
                        let data = buf.copy_to_bytes(len);
                        buf.resize(udp_buffer_size, 0);

                        if let anyhow::Result::<_>::Err(_) = try {
                            send_stream.write_varint(len as u32).await?;
                            send_stream.write_all(&data).await?;
                        } {
                            log::info!(
                                "udp data stream `{}` disconnect. (route: `{}` udp)",
                                remote_address,
                                route_name
                            );
                            break;
                        }
                    }
                    log::info!("udp route `{}` close", &route_name);
                });
            }
        }
    }

    ep.wait_idle().await;
    log::info!("client closed");

    Ok(())
}
