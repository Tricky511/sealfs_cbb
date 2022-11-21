use self::enginerpc::{
    enginerpc_server::Enginerpc, enginerpc_server::EnginerpcServer, EngineRequest, EngineResponse,
};

use crate::storage_engine::StorageEngine;
use tonic::{Request, Response, Status};
use std::sync::Arc;
pub mod enginerpc {
    tonic::include_proto!("enginerpc");
}

pub struct RPCService<Storage: StorageEngine> {
    pub local_storage: Arc<Storage>,
}

pub fn new_manager_service<
    Storage: StorageEngine + std::marker::Send + std::marker::Sync + 'static,
>(
    service: RPCService<Storage>,
) -> EnginerpcServer<RPCService<Storage>> {
    EnginerpcServer::new(service)
}
impl<Storage: StorageEngine> RPCService<Storage> {
    pub fn new(local_storage: Arc<Storage>) -> Self {
        Self { local_storage }
    }
}
#[tonic::async_trait]
impl<Storage: StorageEngine + std::marker::Send + std::marker::Sync + 'static> Enginerpc
    for RPCService<Storage>
{
    async fn directory_add_entry(
        &self,
        request: Request<EngineRequest>,
    ) -> Result<Response<EngineResponse>, Status> {
        let message = request.get_ref();
        println!("add_entry {:?}", message);
        let result = self
            .local_storage
            .directory_add_entry(message.parentdir.clone(), message.filename.clone());
        let status = match result {
            Ok(()) => 0,
            Err(value) => value.into(),
        };
        let response = EngineResponse { status };
        Ok(Response::new(response))
    }
    async fn directory_delete_entry(
        &self,
        request: Request<EngineRequest>,
    ) -> Result<Response<EngineResponse>, Status> {
        let message = request.get_ref();
        println!("del_entry {:?}", message);
        let result = self
            .local_storage
            .directory_delete_entry(message.parentdir.clone(), message.filename.clone());
        let status = match result {
            Ok(()) => 0,
            Err(value) => value.into(),
        };
        let response = EngineResponse { status };
        Ok(Response::new(response))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use crate::storage_engine::{StorageEngine, default_engine::DefaultEngine};
    use super::{RPCService, enginerpc::enginerpc_client::EnginerpcClient};
    use crate::{DistributedEngine, EngineError};
    use common::distribute_hash_table::build_hash_ring;
    use tokio::time::sleep;

    async fn add_server(database_path: String, storage_path: String, local_distributed_address: String) -> Arc<DistributedEngine<DefaultEngine>>{
        let local_storage = Arc::new(DefaultEngine::new(&database_path, &storage_path));
        local_storage.init();
        let service = RPCService::new(local_storage.clone());
        let local = local_distributed_address.clone();
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(super::new_manager_service(service))
                .serve((&local).parse().unwrap())
                .await
        });

        Arc::new(DistributedEngine::new(
            local_distributed_address,
            local_storage,
        ))
    }

    #[tokio::test]
    async fn test_file() {
        build_hash_ring(vec!["127.0.0.1:8080".into(), "127.0.0.1:8081".into()]);
        let engine0 = add_server("/tmp/test_file_db0".into(), "/tmp/test0".into(), "127.0.0.1:8080".into()).await;
        add_server("/tmp/test_file_db1".into(), "/tmp/test1".into(), "127.0.0.1:8081".into()).await;
        let arc_engine0 = engine0.clone();
        tokio::spawn(async move {
            let connection = EnginerpcClient::connect("http://127.0.0.1:8081").await.unwrap();
            arc_engine0.add_connection("127.0.0.1:8080".into(), None);
            arc_engine0.add_connection("127.0.0.1:8081".into(), Some(connection));
            println!("connected");
        });
        sleep(std::time::Duration::from_millis(5000)).await;
        engine0.delete_file("/test".into()).await.unwrap();
        
        // end with '/'
        match engine0.create_file("/test/".into()).await {
            Err(EngineError::IsDir) => assert!(true),
            _ => assert!(false),
        };
        match engine0.create_file("/test".into()).await {
            Ok(()) => assert!(true),
            _ => assert!(false),
        };
        // repeat the same file
        match engine0.create_file("/test".into()).await {
            Err(EngineError::Exist) => assert!(true),
            _ => assert!(false),
        };
        match engine0.delete_file("/test".into()).await {
            Ok(()) => assert!(true),
            _ => assert!(false),
        };
    }

    #[tokio::test]
    async fn test_dir() {
        build_hash_ring(vec!["127.0.0.1:8082".into(), "127.0.0.1:8083".into()]);
        let engine0 = add_server("/tmp/test_dir_db0".into(), "/tmp/test2".into(), "127.0.0.1:8082".into()).await;
        add_server("/tmp/test_dir_db1".into(), "/tmp/test3".into(), "127.0.0.1:8083".into()).await;
        let arc_engine0 = engine0.clone();
        tokio::spawn(async move {
            let connection = EnginerpcClient::connect("http://127.0.0.1:8083").await.unwrap();
            arc_engine0.add_connection("127.0.0.1:8082".into(), None);
            arc_engine0.add_connection("127.0.0.1:8083".into(), Some(connection));
            println!("connected");
        });
        sleep(std::time::Duration::from_millis(5000)).await;
        engine0.delete_file("/test/t1".into()).await.unwrap();
        engine0.delete_dir("/test/".into()).await.unwrap();

        // not end with '/'
        match engine0.create_dir("/test".into()).await {
            Err(EngineError::NotDir) => assert!(true),
            _ => assert!(false),
        };
        match engine0.create_dir("/test/".into()).await {
            core::result::Result::Ok(()) => assert!(true),
            _ => assert!(false),
        };
        // repeat the same dir
        match engine0.create_dir("/test/".into()).await {
            Err(EngineError::Exist) => assert!(true),
            _ => assert!(false),
        };
        // dir add file
        match engine0.create_file("/test/t1".into()).await {
            Ok(()) => assert!(true),
            _ => assert!(false),
        };
        // dir has file
        match engine0.delete_dir("/test/".into()).await {
            Err(EngineError::NotEmpty) => assert!(true),
            _ => assert!(false),
        };
        match engine0.delete_file("/test/t1".into()).await {
            Ok(()) => assert!(true),
            _ => assert!(false),
        };
        match engine0.delete_dir("/test/".into()).await {
            Ok(()) => assert!(true),
            _ => assert!(false),
        };
    }
}