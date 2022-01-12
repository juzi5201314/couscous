use bincode::{Decode, Encode};
use integer_encoding::VarIntAsyncReader;
use once_cell::sync::Lazy;
use quinn::{RecvStream, SendStream};
use smallvec::SmallVec;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::config::RouteType;

pub static BINCODE_CONFIG: Lazy<bincode::config::Configuration> =
    Lazy::new(bincode::config::Configuration::standard);

pub const CODE_SHUTDOWN: u8 = 0;
pub const CODE_AUTH_FAILED: u8 = 10;

pub const CODE_AUTH_SUCCESS: u8 = 11;

#[derive(Encode, Decode, PartialEq, Debug)]
pub struct Auth {
    pub token: String,
}

#[derive(Encode, Decode, PartialEq, Debug)]
pub struct StreamStart {
    pub route_name: String,
}

#[derive(Encode, Decode, PartialEq, Debug)]
pub struct RegisterRoute {
    pub name: String,
    pub _type: RouteType,
}

#[derive(Encode, Decode, PartialEq, Debug)]
pub enum RegisterRouteRes {
    Ok,
    Err(RegisterRouteError),
}

#[derive(Encode, Decode, PartialEq, Debug)]
pub enum RegisterRouteError {
    Repeated(RegisterRoute),
    RouteNotFound(RegisterRoute),
    Other(String, RegisterRoute),
}

#[inline]
pub async fn read_proto<T, const C: usize>(stream: &mut RecvStream) -> anyhow::Result<T>
where
    T: Decode,
{
    let mut buf = SmallVec::<[u8; C]>::new_const();
    let buf_len = stream.read_varint_async().await?;
    buf.reserve_exact(buf_len);
    unsafe {
        buf.set_len(buf_len);
    }

    stream.read_exact(&mut buf).await?;

    Ok(bincode::decode_from_std_read::<T, _, _>(
        &mut &*buf,
        *BINCODE_CONFIG,
    )?)
}

#[inline]
pub async fn write_proto<T, const C: usize>(stream: &mut SendStream, val: T) -> anyhow::Result<()>
where
    T: Encode,
{
    let mut buf = SmallVec::<[u8; C]>::new_const();
    bincode::encode_into_std_write(val, &mut buf, *BINCODE_CONFIG)?;
    stream.write_varint(buf.len() as u32).await?;
    stream.write_all(&buf).await?;
    Ok(())
}

#[async_trait::async_trait]
pub trait VarIntWriter: AsyncWrite + Unpin {
    #[cfg(target_feature = "sse2")]
    async fn write_varint<VI: varint_simd::VarIntTarget + Send>(
        &mut self,
        n: VI,
    ) -> std::io::Result<usize> {
        let (buf, n) = unsafe { varint_simd::encode_unsafe(n) };
        self.write_all(&buf[0..n as usize]).await?;
        Ok(n as usize)
    }

    #[cfg(not(target_feature = "sse2"))]
    #[inline]
    async fn write_varint<VI: integer_encoding::VarInt + Send>(
        &mut self,
        n: VI,
    ) -> std::io::Result<usize> {
        integer_encoding::VarIntAsyncWriter::write_varint_async(self, n)
    }
}

#[cfg(target_feature = "sse2")]
#[async_trait::async_trait]
impl<AW: AsyncWrite + Send + Unpin> VarIntWriter for AW {}
