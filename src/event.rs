use tracing_core::span::Id;

use std::time::SystemTime;

pub enum Event {
    Spawn {
        id: Id,
        time: SystemTime,
        fields: String,
    },
    Enter {
        id: Id,
        time: SystemTime,
    },
    Exit {
        id: Id,
        time: SystemTime,
    },
    Close {
        id: Id,
        time: SystemTime,
    },
}

impl Event {
    pub fn spawn(id: Id, fields: String) -> Self {
        Self::Spawn {
            id,
            time: SystemTime::now(),
            fields,
        }
    }

    pub fn enter(id: Id) -> Self {
        Self::Enter {
            id,
            time: SystemTime::now(),
        }
    }

    pub fn exit(id: Id) -> Self {
        Self::Exit {
            id,
            time: SystemTime::now(),
        }
    }

    pub fn close(id: Id) -> Self {
        Self::Close {
            id,
            time: SystemTime::now(),
        }
    }
}