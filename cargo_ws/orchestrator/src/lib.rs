// Prevents warning from unused imports in generated messages
#[allow(unused_imports)]
mod generated_messages {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}
pub use generated_messages::*;

use geometry_msgs::{Twist, Vector3};
use log::info;
use roslibrust::{Publish, Subscribe, TopicProvider, traits::Ros};
use std_msgs::Bool;

const PUBLISH_PERIOD: std::time::Duration = std::time::Duration::from_millis(50); // f: 20hz

#[derive(Default)]
pub struct OrchestratorState {
    pub stop_requested: bool,
    pub agent_goal_active: bool,
    pub latest_agent_cmd: Option<Twist>,
    pub latest_nav_cmd: Option<Twist>,
}

impl OrchestratorState {
    /// Select the velocity command that should be published for the current state.
    pub fn velocity_filter(&self) -> Option<Twist> {
        if self.stop_requested {
            if self.agent_goal_active {
                if let Some(ref t) = self.latest_agent_cmd {
                    info!("STOP active + agent goal -> forwarding agent cmd_vel");
                    Some(t.clone())
                } else {
                    info!("STOP active + agent goal -> waiting for agent cmd_vel");
                    None
                }
            } else {
                info!("STOP active + no agent goal -> publishing zero");
                Some(Twist::default())
            }
        } else {
            if self.agent_goal_active
                && let Some(ref t) = self.latest_agent_cmd
            {
                info!("No stop + agent goal -> forwarding agent cmd_vel");
                Some(t.clone())
            } else if !self.agent_goal_active
                && let Some(ref t) = self.latest_nav_cmd
            {
                info!("No stop + no agent goal -> forwarding nav cmd_vel");
                Some(t.clone())
            } else {
                None
            }
        }
    }
}

pub struct TopicInterfaces<T: TopicProvider> {
    nav_cmd_vel: T::Subscriber<Twist>,
    agent_cmd_vel: T::Subscriber<Twist>,
    gwendolen_control: T::Subscriber<Bool>,
    gwendolen_goal: T::Subscriber<Vector3>,
    cmd_vel_filtered: T::Publisher<Twist>,
}

pub struct OrchestratorNode<T: Ros> {
    pub ros: T,
    state: OrchestratorState,
}

impl<T: Ros> OrchestratorNode<T> {
    pub fn new(ros: T) -> Self {
        Self {
            ros,
            state: OrchestratorState::default(),
        }
    }

    /// Initialises publishers and subscribers.
    ///
    /// This is separate from `run()` so callers can guarantee the interfaces are listening
    /// before spawning `run()`. Otherwise `run()` could be spawned and a topic written to before
    /// the task has been polled for the first time.
    pub async fn advertise_and_subscribe(&self) -> TopicInterfaces<T> {
        TopicInterfaces {
            nav_cmd_vel: self.ros.subscribe::<Twist>("/nav/cmd_vel").await.unwrap(),
            agent_cmd_vel: self.ros.subscribe::<Twist>("/agent/cmd_vel").await.unwrap(),
            gwendolen_control: self
                .ros
                .subscribe::<Bool>("/gwendolen_control")
                .await
                .unwrap(),
            gwendolen_goal: self
                .ros
                .subscribe::<Vector3>("/gwendolen_goal")
                .await
                .unwrap(),
            cmd_vel_filtered: self
                .ros
                .advertise::<Twist>("/cmd_vel_filtered")
                .await
                .unwrap(),
        }
    }

    /// Runs the node's control loop until the process exits.
    pub async fn run(mut self, mut topic_interfaces: TopicInterfaces<T>) {
        let mut clock = tokio::time::interval(PUBLISH_PERIOD);
        // Skip missed ticks rather than bursting to catch up: a stalled loop must not replay
        // stale commands back-to-back on the output topic.
        clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        info!("Orchestrator node running...");
        loop {
            tokio::select! { // Handy Rust Async macro, will run whichever branch's precondition
                             // evaluates to true first
                biased; // Indicates to the macro that the precondition's should be polled in the
                        // order they appear. The clock is polled first so a continuously-ready
                        // subscriber cannot starve publishing.
                _ = clock.tick() => {
                     if let Some(filtered) = self.state.velocity_filter() && let Err(err) = topic_interfaces.cmd_vel_filtered.publish(&filtered).await {
                            info!("Error with pubisher on /nav/cmd_vel_filtered:\n{err}");
                     }
                },
                msg = topic_interfaces.gwendolen_control.next() => match msg{
                    Ok(msg) => {
                        self.state.stop_requested = msg.data;
                        info!("Safety stop signal: {}", self.state.stop_requested);
                    },
                    Err(err) => info!("Error with subscriber to /gwendolen_control:\n{err}")
                },
                msg = topic_interfaces.gwendolen_goal.next() => match msg{
                    Ok(msg) => {
                        self.state.agent_goal_active = true;
                        self.state.latest_agent_cmd = None; // Reset previous cmd_vel
                        info!("Agent goal received -> active (x={}, y={}, z={})", msg.x, msg.y, msg.z);
                    },
                    Err(err) => info!("Error with subscriber to /gwendolen_goal:\n{err}")
                },
                msg = topic_interfaces.agent_cmd_vel.next() => match msg{
                    Ok(msg) => {
                        self.state.latest_agent_cmd = Some(msg);
                    },
                    Err(err) => info!("Error with subscriber to /agent/cmd_vel:\n{err}")
                },
                msg = topic_interfaces.nav_cmd_vel.next() => match msg{
                    Ok(msg) => {
                        self.state.latest_nav_cmd = Some(msg);
                    },
                    Err(err) => info!("Error with subscriber to /nav/cmd_vel:\n{err}")
                },
            }
        }
    }
}
