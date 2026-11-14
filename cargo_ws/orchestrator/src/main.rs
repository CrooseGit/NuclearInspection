use log::info;
use orchestrator::OrchestratorNode;

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Orchestrator Node Starting...");

    // Connect directly to ros1
    let ros = roslibrust::ros1::NodeHandle::new("http://localhost:11311", "/rust_orchestrator")
        .await
        .unwrap();

    let orchestrator_node = OrchestratorNode::new(ros);
    let topic_interfaces = orchestrator_node.advertise_and_subscribe().await;
    tokio::spawn(async move { orchestrator_node.run(topic_interfaces).await });
    tokio::signal::ctrl_c().await.unwrap();
    info!("Orchestrator Node Shutting Down...");
}
