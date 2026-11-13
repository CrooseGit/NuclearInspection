include!(concat!(env!("OUT_DIR"), "/messages.rs"));
use log::info;
use roslibrust::{Publish, Subscribe, traits::Ros};

const PUBLISH_PERIOD: std::time::Duration = std::time::Duration::from_millis(50); // f: 20hz

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Orchestrator Node Starting...");

    // Connect directly to ros1
    let ros = roslibrust::ros1::NodeHandle::new("http://localhost:11311", "/rust_orchestrator")
        .await
        .unwrap();

    let orchestrator_node = OrchestratorNode {
        ros,
        stop_requested: false,
        agent_goal_active: false,
        latest_agent_cmd: None,
        latest_nav_cmd: None,
    };

    tokio::spawn(async move { orchestrator_node.run().await });
    tokio::signal::ctrl_c().await.unwrap();
    info!("Orchestrator Node Shutting Down...");
}

struct OrchestratorNode<T: Ros> {
    ros: T,
    stop_requested: bool,
    agent_goal_active: bool,
    latest_agent_cmd: Option<geometry_msgs::Twist>,
    latest_nav_cmd: Option<geometry_msgs::Twist>,
}

impl<T: Ros> OrchestratorNode<T> {
    fn orchestrator(&self) -> Option<geometry_msgs::Twist> {
        if self.stop_requested {
            if self.agent_goal_active {
                if let Some(ref t) = self.latest_agent_cmd {
                    info!("STOP active + agent goal -> forwarding agent cmd_vel");
                    return Some(t.clone());
                } else {
                    info!("STOP active + agent goal -> waiting for agent cmd_vel");
                    return None;
                }
            } else {
                info!("STOP active + no agent goal -> publishing zero");
                return Some(geometry_msgs::Twist::default());
            }
        } else {
            if self.agent_goal_active
                && let Some(ref t) = self.latest_agent_cmd
            {
                info!("No stop + agent goal -> forwarding agent cmd_vel");
                return Some(t.clone());
            } else if !self.agent_goal_active
                && let Some(ref t) = self.latest_nav_cmd
            {
                info!("No stop + no agent goal -> forwarding nav cmd_vel");
                return Some(t.clone());
            }
        }
        return None;
    }

    async fn run(mut self) {
        let mut subscriber_nav_cmd_vel = self
            .ros
            .subscribe::<geometry_msgs::Twist>("/nav/cmd_vel")
            .await
            .unwrap();
        let mut subscriber_agent_cmd_vel = self
            .ros
            .subscribe::<geometry_msgs::Twist>("/agent/cmd_vel")
            .await
            .unwrap();
        let mut subscriber_gwendolen_control = self
            .ros
            .subscribe::<std_msgs::Bool>("/gwendolen_control")
            .await
            .unwrap();
        let mut subscriber_gwendolen_goal = self
            .ros
            .subscribe::<geometry_msgs::Vector3>("/gwendolen_goal")
            .await
            .unwrap();
        //TODO: set queue_size to 10
        let publisher_cmd_vel_filtered = self
            .ros
            .advertise::<geometry_msgs::Twist>("/cmd_vel_filtered")
            .await
            .unwrap();

        let mut clock = tokio::time::interval(PUBLISH_PERIOD);
        loop {
            tokio::select! { // Handy Rust Async macro, will run whichever branch's precondition
                             // evaluates to true first
                biased; // Indicates to the macro that the precondition's should be polled in the
                        // order they appear
                msg = subscriber_gwendolen_control.next() => match msg{
                    Ok(msg) => {
                        self.stop_requested = msg.data;
                        info!("Safety stop signal: {}", self.stop_requested);
                    },
                    Err(err) => info!("Error with subscriber to /gwendolen_control:\n{err}")
                },
                msg = subscriber_gwendolen_goal.next() => match msg{
                    Ok(msg) => {
                        self.agent_goal_active = true;
                        self.latest_agent_cmd = None; // Reset previous cmd_vel
                        info!("Agent goal received -> active (x={}, y={}, z={})", msg.x, msg.y, msg.z);
                    },
                    Err(err) => info!("Error with subscriber to /gwendolen_goal:\n{err}")
                },
                msg = subscriber_agent_cmd_vel.next() => match msg{
                    Ok(msg) => {
                        self.latest_agent_cmd = Some(msg);
                    },
                    Err(err) => info!("Error with subscriber to /agent/cmd_vel:\n{err}")
                },
                msg = subscriber_nav_cmd_vel.next() => match msg{
                    Ok(msg) => {
                        self.latest_nav_cmd = Some(msg);
                    },
                    Err(err) => info!("Error with subscriber to /nav/cmd_vel:\n{err}")
                },
                _ = clock.tick() => {
                     if let Some(f_t) = self.orchestrator() &&  let Err(err) = publisher_cmd_vel_filtered.publish(&f_t).await {
                            info!("Error with pubisher on /nav/cmd_vel_filtered:\n{err}");
                     }
                }
            }
        }
    }
}
