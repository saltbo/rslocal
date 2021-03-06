use std::sync::Arc;

use log::{debug, info};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Sender};
use tonic::transport::Server;
use crate::server::{Config, HttpServer, MakeHttpServer, Payload, RSLServer, RSLUser, TcpServer};
use crate::server::api::tunnel_server::TunnelServer;
use crate::server::api::user_server::UserServer;

pub struct Tunnel {
    cfg: Config,

    tcp_server: TcpServer,
    http_server: HttpServer,
}

impl Tunnel {
    pub fn new(cfg: Config) -> Self {
        let http_cfg = cfg.http.clone();
        Tunnel {
            cfg,
            tcp_server: TcpServer::new(),
            http_server: HttpServer::new(http_cfg),
        }
    }

    fn start_http_svc(&self) {
        debug!("start http-server");
        let cfg = self.cfg.clone();
        let http_server_inner = Arc::clone(&self.http_server.inner);
        tokio::spawn(async move {
            let addr = cfg.http.bind_addr.parse().unwrap();
            let server = hyper::Server::bind(&addr)
                .http1_preserve_header_case(true)
                .http1_title_case_headers(true)
                .serve(MakeHttpServer { http_server: HttpServer { inner: http_server_inner } });

            info!("http server listening on //{}", addr);
            if let Err(e) = server.await {
                eprintln!("server error: {}", e);
            }
        });
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let (tx1, mut rx1) = mpsc::channel(128);
        let http_server_inner = self.http_server.inner.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx1.recv().await {
                http_server_inner.lock().await.event_handler(msg).await;
            }
        });

        let (tx2, mut rx2) = mpsc::channel(128);
        let mut tcp_server = self.tcp_server.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx2.recv().await {
                tcp_server.event_handler(msg).await;
            }
        });

        self.start_http_svc();
        self.run_grpc_svc(tx1, tx2).await
    }

    async fn run_grpc_svc(&self, tx_http: Sender<Payload>, tx_tcp: Sender<Payload>) -> anyhow::Result<()> {
        debug!("run_grpc_svc");
        let cfg = self.cfg.clone();
        let addr = cfg.core.bind_addr.parse()?;
        let user = RSLUser::new(cfg.clone());
        let tunnel = RSLServer::new(cfg, tx_tcp, tx_http);

        info!("grpc server listening on //{}", addr);
        Server::builder()
            .add_service(UserServer::new(user.clone()))
            .add_service(TunnelServer::with_interceptor(tunnel, user))
            .serve(addr)
            .await?;
        Ok(())
    }
}


// async fn event_loop<T>(callback: impl Fn(T) + Send + 'static) -> Sender<T>
//     where
//         T: Send + 'static
// {
//     let (tx, mut rx) = mpsc::channel(128);
//     tokio::spawn(async move {
//         while let Some(msg) = rx.recv().await {
//             callback(msg)
//         }
//     });
//     tx
// }
// async fn key_recv<V>(key: String) -> V {
//     let (tx, mut rx) = mpsc::channel(128);
//     VHOST.lock().unwrap().insert(key, tx);
//     rx.recv().await.unwrap()
// }
//
// async fn key_send<V>(key: String, v: V) {
//     let tx = VHOST.lock().unwrap().get(key.as_str()).unwrap();
//     tx.send(v).await;
// }

