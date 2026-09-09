pub mod mpv;
pub mod protocol;

pub use mpv::{run_mpv_actor, MpvSupervisor};
pub use protocol::{MpvCommand, MpvEvent};
