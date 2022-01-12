use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::{Buf, BytesMut};
use fnv::FnvHashMap;
use integer_encoding::VarIntAsyncReader;
use quinn::{Connecting, Connection, ConnectionError, Endpoint, SendStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio_graceful_shutdown::{SubsystemHandle, Toplevel};

use crate::config::{RouteType, ServerConfig};
use crate::proto::{
    read_proto, write_proto, Auth, RegisterRoute, RegisterRouteError, RegisterRouteRes,
    StreamStart, VarIntWriter, CODE_AUTH_FAILED, CODE_AUTH_SUCCESS, CODE_SHUTDOWN,
};
use crate::quic::{transport_config, QuicStream};

pub async fn run(config: Arc<ServerConfig>) -> anyhow::Result<()> {
    let (ep, mut incoming) = Endpoint::server(
        {
            let mut c = quinn::ServerConfig::with_single_cert(
                config.cert.clone(),
                config.private_key.clone(),
            )?;
            c.transport = Arc::new(transport_config());
            c
        },
        config.bind,
    )?;

    log::info!("listen on {}", ep.local_addr()?);

    while let Some(connection) = incoming.next().await {
        let config = Arc::clone(&config);
        tokio::spawn(async move {
            Toplevel::new()
                .start("handle connecting", |h| handle_conn(connection, config, h))
                .handle_shutdown_requests(Duration::from_secs(3))
                .await
                .unwrap();
        });
    }

    tokio::time::timeout(Duration::from_secs(5), ep.wait_idle())
        .await
        .unwrap();
    ep.close(CODE_SHUTDOWN.into(), &[]);
    log::info!("server closed");

    Ok(())
}

async fn handle_conn(
    connection: Connecting,
    config: Arc<ServerConfig>,
    sys_handle: SubsystemHandle,
) -> anyhow::Result<()> {
    let remote_addr = connection.remote_address();
    log::info!("client {} connecting", remote_addr);
    let mut new_conn = connection.await?;
    let mut handshake_stream = new_conn
        .bi_streams
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("handshake_stream is missing"))??;

    // authorization
    let auth = read_proto::<Auth, 32>(&mut handshake_stream.1).await?;
    if auth.token != config.token {
        log::error!("client {} authorization failed", remote_addr);
        new_conn.connection.close(CODE_AUTH_FAILED.into(), &[]);
        return Ok(());
    } else {
        log::debug!("authentication success");
        handshake_stream.0.write_u8(CODE_AUTH_SUCCESS).await?;
    }

    let conn = Arc::new(new_conn.connection);

    let handle = sys_handle.clone();
    tokio::spawn(async move {
        if let Some(Err(ConnectionError::TimedOut)) = new_conn.uni_streams.next().await {
            log::info!("client `{}` timeout", remote_addr);
            handle.request_shutdown()
        }
    });

    // exchange routing information
    let routes = read_proto::<Vec<RegisterRoute>, 1024>(&mut handshake_stream.1).await?;

    #[allow(clippy::never_loop)]
    'a: loop {
        for route in routes {
            if let res @ RegisterRouteRes::Err(_) =
                register_route(&config, route, &conn, sys_handle.clone()).await
            {
                write_proto::<_, 32>(&mut handshake_stream.0, res).await?;
                break 'a;
            }
        }
        write_proto::<_, 4>(&mut handshake_stream.0, RegisterRouteRes::Ok).await?;
        break;
    }

    handshake_stream.0.finish().await?;

    Ok(())
}

async fn register_route(
    config: &ServerConfig,
    register_route: RegisterRoute,
    conn: &Arc<Connection>,
    sys_handle: SubsystemHandle,
) -> RegisterRouteRes {
    if let Some(r) = config
        .route
        .get(&*register_route.name)
        .filter(|r| r._type == register_route._type)
    {
        let conn = Arc::clone(conn);
        if let Err(err) = match register_route._type {
            RouteType::Tcp => {
                build_tcp_route(
                    conn,
                    r.bind,
                    register_route.name.clone(),
                    sys_handle.clone(),
                )
                .await
            }
            RouteType::Udp => {
                build_udp_route(
                    conn,
                    r.bind,
                    register_route.name.clone(),
                    sys_handle.clone(),
                    r.udp_buffer.unwrap_or(2048),
                )
                .await
            }
        } {
            if matches!(err.kind(), std::io::ErrorKind::AddrInUse) {
                RegisterRouteRes::Err(RegisterRouteError::Repeated(register_route))
            } else {
                RegisterRouteRes::Err(RegisterRouteError::Other(err.to_string(), register_route))
            }
        } else {
            log::info!(
                "route {}({:?}) registered",
                &register_route.name,
                register_route._type
            );
            RegisterRouteRes::Ok
        }
    } else {
        RegisterRouteRes::Err(RegisterRouteError::RouteNotFound(register_route))
    }
}

async fn build_tcp_route(
    conn: Arc<Connection>,
    addr: SocketAddr,
    route_name: String,
    sys_handle: SubsystemHandle,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tokio::spawn(async move {
        log::info!("tcp route listen on {}", addr);

        loop {
            tokio::select! {
                biased;

                Ok((tcp_stream, _addr)) = listener.accept() => {
                    let conn = conn.clone();
                    let route_name = route_name.clone();
                    tokio::spawn(async move {
                        let (mut send_stream, recv_stream) = conn.open_bi().await.unwrap();
                        write_proto::<_, 32>(
                            &mut send_stream,
                            StreamStart {
                                route_name: route_name.clone(),
                            },
                        )
                        .await
                        .unwrap();

                        let mut quic_stream = tokio::io::BufStream::new(QuicStream {
                            bi: (send_stream, recv_stream),
                        });
                        let mut tcp_stream = tokio::io::BufStream::new(tcp_stream);

                        tokio::io::copy_bidirectional(&mut tcp_stream, &mut quic_stream).await.ok();

                        log::info!("tcp stream `{}` disconnect. (route: `{}` tcp)", _addr, route_name);
                    });
                }
                _ = sys_handle.on_shutdown_requested() => {
                    break
                }
            }
        }
        log::info!("tcp route `{}` close", addr);
    });

    Ok(())
}

async fn build_udp_route(
    conn: Arc<Connection>,
    addr: SocketAddr,
    route_name: String,
    sys_handle: SubsystemHandle,
    udp_buffer_size: usize,
) -> std::io::Result<()> {
    let socket = Arc::new(UdpSocket::bind(addr).await?);
    tokio::spawn(async move {
        log::info!("udp route listen on {}", addr);

        let mut socket_streams = FnvHashMap::<_, SendStream>::default();
        let (tx, mut rx) = tokio::sync::mpsc::channel(5);

        let mut buf = BytesMut::with_capacity(udp_buffer_size);
        buf.resize(udp_buffer_size, 0);

        loop {
            tokio::select! {
                biased;

                Some(addr) = rx.recv() => {
                    socket_streams.remove(&addr);
                }

                Ok((len, addr)) = socket.recv_from(&mut buf) => {
                    if let Some(send_stream) = socket_streams.get_mut(&addr) {
                        if let anyhow::Result::<_>::Err(_) = try {
                            send_stream.write_varint(len as u32).await?;
                            send_stream.write_all(buf.copy_to_bytes(len).as_ref()).await?;
                        } {
                            socket_streams.remove(&addr);
                        }
                    } else {
                        let (send_stream, recv_stream) = match try {
                            let (mut send_stream, recv_stream) = conn.open_bi().await?;
                            write_proto::<_, 32>(
                                &mut send_stream,
                                StreamStart {
                                    route_name: route_name.clone(),
                                },
                            )
                            .await?;
                            send_stream.write_varint(len as u32).await?;
                            send_stream.write_all(buf.copy_to_bytes(len).as_ref()).await?;
                            (send_stream, recv_stream)
                        } {
                            anyhow::Result::<_>::Err(err) => {
                                log::debug!("error sending data for the first time: {:?}", err);
                                continue
                            }
                            Ok(o) => o,
                        };
                        socket_streams.insert(addr, send_stream);

                        let socket = Arc::clone(&socket);
                        let conn = Arc::clone(&conn);
                        let tx = tx.clone();
                        let route_name = route_name.clone();
                        tokio::spawn(async move {
                            let mut buf = BytesMut::with_capacity(udp_buffer_size);
                            let mut buf_reader = tokio::io::BufReader::new(recv_stream);
                            loop {
                                if let anyhow::Result::<_>::Err(_) = try {
                                    let len = buf_reader.read_varint_async().await?;
                                    buf.resize(len, 0);
                                    buf_reader.read_exact(&mut buf).await?;
                                    socket.send_to(&buf.copy_to_bytes(len), addr).await?;
                                } {
                                    tx.send(addr).await.ok();
                                    log::info!("udp data stream `{}` disconnect. (route: `{}` udp)", conn.remote_address(), route_name);
                                    break
                                }
                            }
                        });
                    }
                    buf.resize(udp_buffer_size, 0);
                }

                _ = sys_handle.on_shutdown_requested() => {
                    break
                }
            }
        }

        log::info!("udp route {} close", addr);
    });

    Ok(())
}
