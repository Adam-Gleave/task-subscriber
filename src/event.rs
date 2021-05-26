use tracing_core::span::Id;

use std::time::SystemTime;

pub enum Event {
    Spawn {
        id: Id,
        at: SystemTime,
        fields: String,
    },
    Enter {
        id: Id,
        at: SystemTime,
    },
    Exit {
        id: Id,
        at: SystemTime,
    },
    Close {
        id: Id,
        at: SystemTime,
    },
}