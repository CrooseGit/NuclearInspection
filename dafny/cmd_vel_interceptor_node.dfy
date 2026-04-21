module {:extern "CmdVelLogic"} CmdVelLogic {

  datatype Decision = UseAgent | UseNav | Stop | Wait

  class Orchestrator { var stopRequested: bool var agentGoalActive: bool
    var agentCmdAvailable: bool var navCmdAvailable: bool

    // Predicate: decision originates from the Safety System
    predicate SSDecision(d: Decision){ d == Decision.UseAgent ||
      d == Decision.Stop  ||   d == Decision.Wait }

    method Decide() returns (d: Decision)

      // COMPLETENESS: method always returns a defined decision
      ensures d == Decision.UseAgent || d == Decision.UseNav ||
              d == Decision.Stop     || d == Decision.Wait

      // SAFETY PRIORITY: under hazardous conditions, only SS decisions
      // are issued — SRAS navigation is never used
      ensures stopRequested ==> SSDecision(d)
      ensures stopRequested ==> d != Decision.UseNav

      // DETERMINISM: UseAgent is only issued when the agent has an
      // active goal and a command ready
      ensures d == Decision.UseAgent ==>
              (agentGoalActive && agentCmdAvailable)

      // DETERMINISM: UseNav is only issued during normal operation,
      // with no active agent goal and a navigation command available
      ensures d == Decision.UseNav ==>
              (!stopRequested && !agentGoalActive && navCmdAvailable)

      // MUTUAL EXCLUSION: UseAgent and UseNav are never issued together
      ensures !(d == Decision.UseAgent && d == Decision.UseNav){
      if stopRequested {
        if agentGoalActive {
          if agentCmdAvailable {
            d := Decision.UseAgent;
          } else {
            d := Decision.Wait;
          }
        } else {
          d := Decision.Stop;
        }
      } else {
        if agentGoalActive && agentCmdAvailable {
          d := Decision.UseAgent;
        } else if !agentGoalActive && navCmdAvailable {
          d := Decision.UseNav;
        } else {
          d := Decision.Wait;
        }
      }
    }
  }
}
