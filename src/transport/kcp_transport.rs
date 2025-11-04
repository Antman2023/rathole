use std::net::SocketAddr;

use super::{AddrMaybeCached, SocketOpts, Transport};
use crate::config::{KcpConfig, TransportConfig};
use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio_kcp::{KcpConfig as TokioKcpConfig, KcpListener, KcpStream, KcpNoDelayConfig};
use tokio::sync::Mutex;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct KcpTransport {
    config: KcpConfig,
}

impl KcpTransport {
    fn build_kcp_config(&self) -> TokioKcpConfig {
        let mut kcp_config = TokioKcpConfig::default();
        kcp_config.mtu = self.config.mtu as usize;
        kcp_config.wnd_size = (self.config.sndwnd as u16, self.config.rcvwnd as u16);
        kcp_config.nodelay = KcpNoDelayConfig {
            nodelay: self.config.nodelay != 0,
            interval: self.config.interval as i32,
            resend: self.config.resend as i32,
            nc: self.config.nc != 0,
        };
        kcp_config
    }
}

#[async_trait]
impl Transport for KcpTransport {
    type Acceptor = Arc<Mutex<KcpListener>>;
    type RawStream = KcpStream;
    type Stream = KcpStream;

    fn new(config: &TransportConfig) -> Result<Self> {
        Ok(KcpTransport {
            config: config.kcp.clone(),
        })
    }

    fn hint(_conn: &Self::Stream, _opt: SocketOpts) {
        // KCP doesn't use socket options like TCP
    }

    async fn bind<T: tokio::net::ToSocketAddrs + Send + Sync>(
        &self,
        addr: T,
    ) -> Result<Self::Acceptor> {
        let kcp_config = self.build_kcp_config();
        let listener = KcpListener::bind(kcp_config, addr)
            .await
            .with_context(|| "Failed to bind KCP listener")?;
        
        Ok(Arc::new(Mutex::new(listener)))
    }

    async fn accept(&self, a: &Self::Acceptor) -> Result<(Self::RawStream, SocketAddr)> {
        let mut listener = a.lock().await;
        let (stream, addr) = listener
            .accept()
            .await
            .with_context(|| "Failed to accept KCP connection")?;
        Ok((stream, addr))
    }

    async fn handshake(&self, conn: Self::RawStream) -> Result<Self::Stream> {
        Ok(conn)
    }

    async fn connect(&self, addr: &AddrMaybeCached) -> Result<Self::Stream> {
        let socket_addr = match addr.socket_addr {
            Some(s) => s,
            None => {
                return Err(anyhow::anyhow!(
                    "Address must be resolved before connecting with KCP"
                ))
            }
        };

        let kcp_config = self.build_kcp_config();
        let stream = KcpStream::connect(&kcp_config, socket_addr)
            .await
            .with_context(|| format!("Failed to connect to {} via KCP", socket_addr))?;

        Ok(stream)
    }
}
