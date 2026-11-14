// Prevents warning from unused imports in generated messages
#[allow(unused_imports)]
mod generated_messages {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}
use generated_messages::*;

use geometry_msgs::{Twist, Vector3};
use log::info;
use roslibrust::{Publish, Subscribe, TopicProvider, traits::Ros};
use std_msgs::Bool;

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
        state: OrchestratorState {
            stop_requested: false,
            agent_goal_active: false,
            latest_agent_cmd: None,
            latest_nav_cmd: None,
        },
    };
    let topic_interfaces = orchestrator_node.advertise_and_subscribe().await;
    tokio::spawn(async move { orchestrator_node.run(topic_interfaces).await });
    tokio::signal::ctrl_c().await.unwrap();
    info!("Orchestrator Node Shutting Down...");
}

struct OrchestratorNode<T: Ros> {
    ros: T,
    state: OrchestratorState,
}
struct OrchestratorState {
    pub stop_requested: bool,
    pub agent_goal_active: bool,
    pub latest_agent_cmd: Option<Twist>,
    pub latest_nav_cmd: Option<Twist>,
}

impl OrchestratorState {
    fn velocity_filter(&self) -> Option<Twist> {
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

struct TopicInterfaces<T: TopicProvider> {
    nav_cmd_vel: T::Subscriber<Twist>,
    agent_cmd_vel: T::Subscriber<Twist>,
    gwendolen_control: T::Subscriber<Bool>,
    gwendolen_goal: T::Subscriber<Vector3>,
    cmd_vel_filtered: T::Publisher<Twist>,
}

impl<T: Ros> OrchestratorNode<T> {
    /// Initialises publishers and subscribers.
    ///
    /// This is separate from `run()` so tests can wire up the interfaces and guarantee they
    /// are listening before spawning `run()`. Otherwise `run()` could be spawned and a topic
    /// written to before the task has been polled for the first time.
    async fn advertise_and_subscribe(&self) -> TopicInterfaces<T> {
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
    async fn run(mut self, mut topic_interfaces: TopicInterfaces<T>) {
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

#[cfg(test)]
mod test {
    use super::*;
    use roslibrust::mock::MockRos;
    use roslibrust::{Publish, Subscribe, TopicProvider};

    /// Receive the next message, panic if nothing is received in a timely manner
    async fn receive_or_panic<S: Subscribe<Twist>>(subscriber: &mut S) -> Twist {
        tokio::time::timeout(std::time::Duration::from_secs(1), subscriber.next())
            .await
            .expect("orchestrator did not publish within 1s")
            .expect("subscriber returned an error")
    }

    /// Assert that nothing is published for 1s
    async fn assert_nothing_published<S: Subscribe<Twist>>(subscriber: &mut S) {
        assert!(
            tokio::time::timeout(tokio::time::Duration::from_secs(1), subscriber.next())
                .await
                .is_err(),
            "orchestrator published unexpectedly"
        );
    }

    /// Clone a twist and set its linear x component.
    fn twist_with_x(base: &Twist, x: f64) -> Twist {
        let mut t = base.clone();
        t.linear.x = x;
        t
    }

    const TWIST_A: Twist = Twist {
        linear: Vector3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        angular: Vector3 {
            x: 3.0,
            y: 2.0,
            z: 1.0,
        },
    };
    const TWIST_B: Twist = Twist {
        linear: Vector3 {
            x: 4.0,
            y: 5.0,
            z: 6.0,
        },
        angular: Vector3 {
            x: 6.0,
            y: 5.0,
            z: 4.0,
        },
    };
    /// Ensure the velocity filter behaves as expected.
    #[test]
    fn orchestrator_logic() {
        let mut state = OrchestratorState {
            stop_requested: false,
            agent_goal_active: false,
            latest_agent_cmd: None,
            latest_nav_cmd: None,
        };

        // No Stop ======

        // Do nothing (Waiting for a cmd)
        assert_eq!(OrchestratorState::velocity_filter(&state), None); // No agent_goal, no agent_cmd, nor nav_cmd

        state.latest_agent_cmd = Some(TWIST_A.clone());
        assert_eq!(OrchestratorState::velocity_filter(&state), None); // No agent_goal nor nav_cmd, agent_cmd set

        state.latest_agent_cmd = None;
        state.agent_goal_active = true;
        assert_eq!(OrchestratorState::velocity_filter(&state), None); // agent_goal set, no agent_cmd

        // FWD NAV
        state.agent_goal_active = false;
        state.latest_nav_cmd = Some(TWIST_B.clone());
        assert_eq!(OrchestratorState::velocity_filter(&state), Some(TWIST_B)); // No agent goal, nav_cmd set

        // FWD AGENT
        state.agent_goal_active = true;
        state.latest_agent_cmd = Some(TWIST_A.clone());
        assert_eq!(
            OrchestratorState::velocity_filter(&state),
            Some(TWIST_A.clone())
        );

        // =========

        // Stop requested ====
        state.stop_requested = true;

        // HALT
        state.agent_goal_active = false;
        assert_eq!(
            OrchestratorState::velocity_filter(&state),
            Some(Twist::default())
        );

        // Do nothing (Waiting for command)
        state.agent_goal_active = true;
        state.latest_agent_cmd = None;
        assert_eq!(OrchestratorState::velocity_filter(&state), None);

        // FWD AGENT
        state.latest_agent_cmd = Some(TWIST_A.clone());
        assert_eq!(OrchestratorState::velocity_filter(&state), Some(TWIST_A));
        // =========
    }

    /// End-to-end test of the node's state machine.
    #[tokio::test]
    async fn e_stop_and_recovery() {
        env_logger::init();
        tokio::time::pause();
        let mock_node = OrchestratorNode {
            ros: MockRos::new(),
            state: OrchestratorState {
                stop_requested: false,
                agent_goal_active: false,
                latest_agent_cmd: None,
                latest_nav_cmd: None,
            },
        };

        let mut subscriber_cmd_vel_filtered = mock_node
            .ros
            .subscribe::<Twist>("/cmd_vel_filtered")
            .await
            .unwrap();

        let publisher_gwendolen_control = mock_node
            .ros
            .advertise::<Bool>("/gwendolen_control")
            .await
            .unwrap();
        let publisher_gwendolen_goal = mock_node
            .ros
            .advertise::<Vector3>("/gwendolen_goal")
            .await
            .unwrap();
        let publisher_agent_cmd_vel = mock_node
            .ros
            .advertise::<Twist>("/agent/cmd_vel")
            .await
            .unwrap();
        let publisher_nav_cmd_vel = mock_node
            .ros
            .advertise::<Twist>("/nav/cmd_vel")
            .await
            .unwrap();

        // Create interfaces before spawning process so our messages are received
        let topic_interfaces = mock_node.advertise_and_subscribe().await;

        // Start node
        tokio::spawn(async move { mock_node.run(topic_interfaces).await });

        // 1. Nominal: nav commands pass straight through.
        for i in 0..5 {
            let t = twist_with_x(&TWIST_A, i as f64);
            publisher_nav_cmd_vel.publish(&t).await.unwrap();
            assert_eq!(receive_or_panic(&mut subscriber_cmd_vel_filtered).await, t);
        }

        // 2. E-stop: nav commands are filtered out and zero velocity is published instead.
        publisher_gwendolen_control
            .publish(&Bool { data: true })
            .await
            .unwrap();
        for i in 0..5 {
            let t = twist_with_x(&TWIST_B, i as f64);
            publisher_nav_cmd_vel.publish(&t).await.unwrap();
            assert_eq!(
                receive_or_panic(&mut subscriber_cmd_vel_filtered).await,
                Twist::default()
            );
        }

        // 3. Recovery: clearing the E-stop lets nav commands through again.
        publisher_gwendolen_control
            .publish(&Bool { data: false })
            .await
            .unwrap();
        for i in 0..5 {
            let t = twist_with_x(&TWIST_A, i as f64);
            publisher_nav_cmd_vel.publish(&t).await.unwrap();
            assert_eq!(receive_or_panic(&mut subscriber_cmd_vel_filtered).await, t);
        }

        // 4. Agent goal with no agent command yet: nav is suppressed, nothing is published.
        publisher_gwendolen_goal
            .publish(&Vector3 {
                x: 5.0,
                y: 5.0,
                z: 5.0,
            })
            .await
            .unwrap();
        for i in 0..5 {
            let t = twist_with_x(&TWIST_B, i as f64);
            publisher_nav_cmd_vel.publish(&t).await.unwrap();
            assert_nothing_published(&mut subscriber_cmd_vel_filtered).await;
        }

        // 5. E-stop with an active goal and agent command: agent command still passes through.
        publisher_gwendolen_control
            .publish(&Bool { data: true })
            .await
            .unwrap();
        for i in 0..5 {
            let t = twist_with_x(&TWIST_A, i as f64);
            publisher_agent_cmd_vel.publish(&t).await.unwrap();
            assert_eq!(receive_or_panic(&mut subscriber_cmd_vel_filtered).await, t);
        }

        // 6. A new goal clears the previous agent command: nothing is forwarded until a fresh
        //    agent command arrives.
        publisher_gwendolen_goal
            .publish(&Vector3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            })
            .await
            .unwrap();
        for i in 0..5 {
            let t = twist_with_x(&TWIST_B, i as f64);
            publisher_nav_cmd_vel.publish(&t).await.unwrap();
            assert_nothing_published(&mut subscriber_cmd_vel_filtered).await;
        }
        for i in 0..5 {
            let t = twist_with_x(&TWIST_A, i as f64);
            publisher_agent_cmd_vel.publish(&t).await.unwrap();
            assert_eq!(receive_or_panic(&mut subscriber_cmd_vel_filtered).await, t);
        }
    }
}
