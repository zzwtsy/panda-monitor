use common::panda_monitor::{
    panda_monitor_server::PandaMonitor, ServerHostRequest, ServerResponse, ServerStateRequest,
    UpdateIpRequest,
};
use tonic::{Request, Response, Status};

#[derive(Default)]
pub struct ServerMonitorService {}

#[tonic::async_trait]
impl PandaMonitor for ServerMonitorService {
    async fn report_server_host(
        &self,
        request: Request<ServerHostRequest>,
    ) -> Result<Response<ServerResponse>, Status> {
        println!("report_server_host: {:?}", request);
        Ok(Response::new(ServerResponse { success: true }))
    }
    async fn report_server_state(
        &self,
        request: Request<ServerStateRequest>,
    ) -> Result<Response<ServerResponse>, Status> {
        println!("report_server_state: {:?}", request);
        Ok(Response::new(ServerResponse { success: true }))
    }

    async fn update_ip(
        &self,
        request: Request<UpdateIpRequest>,
    ) -> Result<Response<ServerResponse>, Status> {
        println!("update_ip: {:?}", request);
        Ok(Response::new(ServerResponse { success: true }))
    }
}
