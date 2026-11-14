use orchestrator::geometry_msgs::{Twist, Vector3};
use orchestrator::std_msgs::Bool;
use orchestrator::{OrchestratorNode, OrchestratorState};
use roslibrust::mock::MockRos;
use roslibrust::{Publish, Subscribe, TopicProvider};

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
        tokio::time::timeout(std::time::Duration::from_secs(1), subscriber.next())
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

/// Ensure the velocity filter behaves as expected.
#[test]
fn orchestrator_logic() {
    let mut state = OrchestratorState::default();

    // No Stop ======

    // Do nothing (Waiting for a cmd)
    assert_eq!(state.velocity_filter(), None); // No agent_goal, no agent_cmd, nor nav_cmd

    state.latest_agent_cmd = Some(TWIST_A.clone());
    assert_eq!(state.velocity_filter(), None); // No agent_goal nor nav_cmd, agent_cmd set

    state.latest_agent_cmd = None;
    state.agent_goal_active = true;
    assert_eq!(state.velocity_filter(), None); // agent_goal set, no agent_cmd

    // FWD NAV
    state.agent_goal_active = false;
    state.latest_nav_cmd = Some(TWIST_B.clone());
    assert_eq!(state.velocity_filter(), Some(TWIST_B)); // No agent goal, nav_cmd set

    // FWD AGENT
    state.agent_goal_active = true;
    state.latest_agent_cmd = Some(TWIST_A.clone());
    assert_eq!(state.velocity_filter(), Some(TWIST_A.clone()));

    // =========

    // Stop requested ====
    state.stop_requested = true;

    // HALT
    state.agent_goal_active = false;
    assert_eq!(state.velocity_filter(), Some(Twist::default()));

    // Do nothing (Waiting for command)
    state.agent_goal_active = true;
    state.latest_agent_cmd = None;
    assert_eq!(state.velocity_filter(), None);

    // FWD AGENT
    state.latest_agent_cmd = Some(TWIST_A.clone());
    assert_eq!(state.velocity_filter(), Some(TWIST_A));
    // =========
}

/// End-to-end test of the node's state machine.
#[tokio::test]
async fn e_stop_and_recovery() {
    env_logger::init();
    tokio::time::pause();
    let mock_node = OrchestratorNode::new(MockRos::new());

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
