//! Sheshnag engine.
//!
//! * **Sakshi** (layer 1) watches and reports. It never changes the machine on its own.
//! * **Kavach** (layer 3) clears, seals and verifies, and can restore.
//! * **Agni** (layer 2) destroys exactly the targets Kavach cleared, and nothing else.
//!
//! Agni can only be reached through [`kavach::ClearedOrder`], which only Kavach can create.

pub mod agni;
pub mod cli;
pub mod engine;
pub mod kavach;
pub mod model;
pub mod patterns;
pub mod platform;
pub mod runner;
pub mod util;

pub use engine::{Engine, Outcome};
pub use model::{Entry, Finding, Proc, Scope, Target};
