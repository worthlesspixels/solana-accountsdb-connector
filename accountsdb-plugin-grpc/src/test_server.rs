use futures_core::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

pub mod accountsdb_proto {
    tonic::include_proto!("accountsdb");
}
use accountsdb_proto::{update::UpdateOneof, AccountWrite, SlotUpdate, SubscribeRequest, Update};

pub mod accountsdb_service {
    use super::*;
    use {
        accountsdb_proto::accounts_db_server::{AccountsDb, AccountsDbServer},
        tokio_stream::wrappers::ReceiverStream,
        tonic::{Request, Response, Status},
    };

    #[derive(Debug)]
    pub struct Service {
        pub sender: broadcast::Sender<Update>,
    }

    impl Service {
        pub fn new() -> Self {
            let (tx, _) = broadcast::channel(100);
            Self { sender: tx }
        }
    }

    #[tonic::async_trait]
    impl AccountsDb for Service {
        type SubscribeStream = ReceiverStream<Result<Update, Status>>;

        async fn subscribe(
            &self,
            _request: Request<SubscribeRequest>,
        ) -> Result<Response<Self::SubscribeStream>, Status> {
            println!("new client");
            let (tx, rx) = mpsc::channel(100);
            let mut broadcast_rx = self.sender.subscribe();
            tokio::spawn(async move {
                loop {
                    // TODO: Deal with lag! maybe just close if RecvError::Lagged happens
                    let msg = broadcast_rx.recv().await.unwrap();
                    tx.send(Ok(msg)).await.unwrap();
                }
            });
            Ok(Response::new(ReceiverStream::new(rx)))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:10000".parse().unwrap();

    let service = accountsdb_service::Service::new();
    let sender = service.sender.clone();
    let svc = accountsdb_proto::accounts_db_server::AccountsDbServer::new(service);

    tokio::spawn(async move {
        loop {
            if sender.receiver_count() > 0 {
                println!("sending...");
                sender
                    .send(Update {
                        update_oneof: Some(UpdateOneof::SlotUpdate(SlotUpdate {
                            slot: 0,
                            parent: None,
                            status: 0,
                        })),
                    })
                    .unwrap();
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    Server::builder().add_service(svc).serve(addr).await?;

    Ok(())
}