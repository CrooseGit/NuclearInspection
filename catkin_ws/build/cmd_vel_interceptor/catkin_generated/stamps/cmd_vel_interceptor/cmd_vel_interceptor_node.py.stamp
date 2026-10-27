#!/usr/bin/env python

import rospy
from geometry_msgs.msg import Twist, Vector3
from std_msgs.msg import Bool

class CmdVelInterceptor:
    def __init__(self):
        rospy.init_node('cmd_vel_interceptor_node', anonymous=True)

        # Internal state
        self.stop_requested = False
        self.agent_goal_active = False
        self.latest_agent_cmd = None
        self.latest_nav_cmd = None

        # Subscribers
        rospy.Subscriber('/agent/cmd_vel', Twist, self.agent_cmd_callback)
        rospy.Subscriber('/nav/cmd_vel', Twist, self.nav_cmd_callback)
        rospy.Subscriber('/gwendolen_control', Bool, self.stop_callback)
        rospy.Subscriber('/gwendolen_goal', Vector3, self.agent_goal_callback)

        # Publisher
        self.cmd_vel_pub = rospy.Publisher('/cmd_vel_filtered', Twist, queue_size=10)

        # Timer to regularly publish cmd_vel
        rospy.Timer(rospy.Duration(0.05), self.publish_cmd)  # 20 Hz
        rospy.loginfo("cmd_vel_interceptor_node running...")

    # --- Callbacks ---
    def stop_callback(self, msg: Bool):
        self.stop_requested = msg.data
        rospy.loginfo(f"Safety stop signal: {self.stop_requested}")

    def agent_cmd_callback(self, msg: Twist):
        self.latest_agent_cmd = msg

    def nav_cmd_callback(self, msg: Twist):
        self.latest_nav_cmd = msg

    def agent_goal_callback(self, msg: Vector3):
        self.agent_goal_active = True
        self.latest_agent_cmd = None  # Reset previous cmd_vel
        rospy.loginfo(f"Agent goal received → active (x={msg.x}, y={msg.y}, z={msg.z})")

    # --- Main arbitration logic ---
    def publish_cmd(self, event):
        twist_to_publish = None

        if self.stop_requested:
            # Stop is active
            if self.agent_goal_active:
                if self.latest_agent_cmd:
                    twist_to_publish = self.latest_agent_cmd
                    rospy.loginfo("STOP active + agent goal → forwarding agent cmd_vel")
                else:
                    twist_to_publish = None  # wait for agent/cmd_vel
                    rospy.loginfo("STOP active + agent goal → waiting for agent cmd_vel")
            else:
                twist_to_publish = Twist()  # zero cmd_vel if no goal
                rospy.logwarn("STOP active + no agent goal → publishing zero")
        else:
            # Stop not active
            if self.agent_goal_active and self.latest_agent_cmd:
                twist_to_publish = self.latest_agent_cmd
                rospy.loginfo("No stop + agent goal → forwarding agent cmd_vel")
            elif not self.agent_goal_active and self.latest_nav_cmd:
                twist_to_publish = self.latest_nav_cmd
                rospy.loginfo("No stop + no agent goal → forwarding nav cmd_vel")

        # Publish only if we have something to publish
        if twist_to_publish is not None:
            self.cmd_vel_pub.publish(twist_to_publish)

if __name__ == '__main__':
    try:
        CmdVelInterceptor()
        rospy.spin()
    except rospy.ROSInterruptException:
        pass
