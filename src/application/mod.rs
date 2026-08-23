//! Application state, normalized actions, effects, errors, and reducer.

mod model;
mod mutations;
mod reducer;

pub use model::{
    Action, AppState, ApplicationError, ApplicationResult, ClipboardIntent, DurabilityState,
    Effect, FailureCode, InteractionMode,
};
pub use reducer::reduce;
