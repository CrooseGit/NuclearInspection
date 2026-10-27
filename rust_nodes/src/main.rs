include!(concat!(env!("OUT_DIR"), "/messages.rs"));
use log::debug;
use roslibrust::{Publish, Subscribe, traits::Ros};
use std::{clone::Clone, sync::Arc};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    env_logger::init();

    // Connect directly to ros1
    let ros = roslibrust::ros1::NodeHandle::new("http://localhost:11311", "/rust_orchestrator")
        .await
        .unwrap();

    let orchestrator_node = OrchestratorNode {
        ros,
        state: Arc::new(Mutex::new(OrchestratorState {
            stop_requested: false,
            agent_goal_active: false,
            latest_agent_cmd: None,
            latest_nav_cmd: None,
        })),
    };

    tokio::spawn(async move { orchestrator_node.run().await });
    tokio::signal::ctrl_c().await.unwrap();
}

struct OrchestratorNode<T: Ros> {
    ros: T,
    state: Arc<Mutex<OrchestratorState>>,
}

struct OrchestratorState {
    stop_requested: bool,
    agent_goal_active: bool,
    latest_agent_cmd: Option<geometry_msgs::Twist>,
    latest_nav_cmd: Option<geometry_msgs::Twist>,
}

impl<T: Ros> OrchestratorNode<T> {
    async fn orchestrator(&self) -> Option<geometry_msgs::Twist> {
        if self.state.lock().await.stop_requested {
            if self.state.lock().await.agent_goal_active {
                if let Some(ref t) = self.state.lock().await.latest_agent_cmd {
                    debug!("STOP active + agent goal -> forwarding agent cmd_vel");
                    return Some(t.clone());
                } else {
                    debug!("STOP active + agent goal -> waiting for agent cmd_vel");
                    return None;
                }
            } else {
                debug!("STOP active + no agent goal -> publishing zero");
                return Some(geometry_msgs::Twist::default());
            }
        } else {
            if self.state.lock().await.agent_goal_active
                && let Some(ref t) = self.state.lock().await.latest_agent_cmd
            {
                debug!("No stop + agent goal -> forwarding agent cmd_vel");
                return Some(t.clone());
            } else if !self.state.lock().await.agent_goal_active
                && let Some(ref t) = self.state.lock().await.latest_nav_cmd
            {
                debug!("No stop + no agent goal -> forwarding nav cmd_vel");
                return Some(t.clone());
            }
        }
        return None;
    }

    async fn run(self) {
        let mut subscriber_nav_cmd_vel = self
            .ros
            .subscribe::<geometry_msgs::Twist>("/nav/cmd_vel")
            .await
            .unwrap();
        let shared_state = Arc::clone(&self.state);
        tokio::spawn(async move {
            loop {
                shared_state.lock().await.latest_nav_cmd =
                    Some(subscriber_nav_cmd_vel.next().await.unwrap());
            }
        });

        let mut subscriber_agent_cmd_vel = self
            .ros
            .subscribe::<geometry_msgs::Twist>("/agent/cmd_vel")
            .await
            .unwrap();
        let shared_state = Arc::clone(&self.state);
        tokio::spawn(async move {
            loop {
                shared_state.lock().await.latest_agent_cmd =
                    Some(subscriber_agent_cmd_vel.next().await.unwrap());
            }
        });

        let mut subscriber_gwendolen_control = self
            .ros
            .subscribe::<std_msgs::Bool>("/gwendolen_control")
            .await
            .unwrap();

        let shared_state = Arc::clone(&self.state);
        tokio::spawn(async move {
            loop {
                shared_state.lock().await.stop_requested =
                    subscriber_gwendolen_control.next().await.unwrap().data;
                debug!(
                    "Safety stop signal: {}",
                    shared_state.lock().await.stop_requested
                );
            }
        });

        let mut subscriber_gwendolen_goal = self
            .ros
            .subscribe::<geometry_msgs::Vector3>("/gwendolen_goal")
            .await
            .unwrap();

        let shared_state = Arc::clone(&self.state);
        tokio::spawn(async move {
            loop {
                let msg = subscriber_gwendolen_goal.next().await.unwrap();
                shared_state.lock().await.agent_goal_active = true;
                shared_state.lock().await.latest_agent_cmd = None;
                debug!(
                    "Agent goal received -> active (x={}, y={}, z={})",
                    msg.x, msg.y, msg.z
                );
            }
        });

        //TODO: set queue_size to 10
        let publisher_cmd_vel_filtered = self
            .ros
            .advertise::<geometry_msgs::Twist>("/cmd_vel_filtered")
            .await
            .unwrap();

        loop {
            if let Some(f_t) = self.orchestrator().await {
                publisher_cmd_vel_filtered.publish(&f_t).await.unwrap();
            }
            //TODO: replace with Const
            tokio::time::sleep(std::time::Duration::from_millis(50)).await; // 20Hz
        }
    }
}
