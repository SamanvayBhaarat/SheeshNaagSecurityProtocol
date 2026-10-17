//! Agni, the fire. It does exactly one thing: destroy the targets in an order Kavach cleared.
//!
//! There are no modes, no options and no second-guessing here, by design. The judgement lives
//! in the order a person confirms and in Kavach's clearance. Agni never opens a network
//! connection and never acts on another computer.
use crate::engine::Engine;
use crate::kavach::ClearedOrder;
use crate::model::Target;

pub struct TargetResult {
    pub target: Target,
    pub outcome: Result<(), String>,
}

pub struct Agni;

impl Agni {
    pub fn execute(engine: &Engine, cleared: &ClearedOrder) -> Vec<TargetResult> {
        let platform = &engine.platform;
        cleared
            .items()
            .iter()
            .map(|item| {
                let target = item.target().clone();
                let outcome = match &target {
                    Target::Persistence { id, label } => {
                        let _ = platform.stop(id, label);
                        platform.remove(id)
                    }
                    Target::Process { pid, .. } => platform.kill(*pid),
                };
                match &outcome {
                    Ok(()) => engine.log(&format!("Agni: destroyed {}", target.describe())),
                    Err(e) => engine.log(&format!("Agni: could not destroy {}: {e}", target.describe())),
                }
                TargetResult { target, outcome }
            })
            .collect()
    }
}
