#![feature(try_blocks)]

use std::path::PathBuf;
use std::sync::Arc;

mod quic;
pub mod config;
pub mod server;
pub mod proto;
pub mod client;

#[global_allocator]
static ALLOC: snmalloc_rs::SnMalloc = snmalloc_rs::SnMalloc;

pub async fn run(conf: PathBuf) -> anyhow::Result<()> {
    let config = crate::config::configuration(conf)?;

    setup_logger(config.log_level)?;

    match (&config.server, &config.client) {
        (None, None) => {
            anyhow::bail!("neither server nor client")
        }
        (Some(_), Some(_)) => {
            anyhow::bail!("cannot be both a server and a client")
        }
        (Some(server), None) => {
            server::run(Arc::new(server.clone())).await
        }
        (None, Some(client)) => {
            let mut retry_num = 0;
            loop {
                if let Err(err) = client::run(Arc::new(client.clone()), &mut retry_num).await {
                    log::error!("{}", err);
                    let time = if let Some(time) = &client.retry_interval {
                        *time.duration()
                    } else {
                        break Ok(())
                    };

                    retry_num += 1;

                    if let Some(max) = client.max_retry {
                        log::info!("start {}/{} retries after {:?}...", retry_num, max, time);
                        if retry_num > max {
                            log::warn!("retry up to the maximum number of times, stop.");
                            break Ok(())
                        }
                    } else {
                        log::info!("start the {}nd retry after {:?}...", retry_num, time);
                    }
                    tokio::time::sleep(time).await;
                }
            }
        }
    }
}

fn setup_logger(level: log::LevelFilter) -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .filter(|metadata| {
            metadata.target() != "tokio_graceful_shutdown::shutdown_token"
        })
        .level(level)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}
