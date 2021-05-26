use crate::event::Event;

use futures::FutureExt;
use tokio::sync::mpsc::Receiver;
use tracing_core::span::Id;

use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

#[derive(Default, Debug)]
struct Task {
    fields: String,
    stats: Stats,
}

#[derive(Default, Debug)]
struct Stats {
    polls: u64,
    current_polls: u64,
    created_at: Option<SystemTime>,
    first_poll: Option<SystemTime>,
    last_poll: Option<SystemTime>,
    busy_time: Duration,
    closed_at: Option<SystemTime>,
}

pub struct Collector {
    rx: Receiver<Event>,
    tasks: HashMap<Id, Task>,
    flush_interval: Duration,
}

impl Collector {
    pub(crate) fn new(rx: Receiver<Event>, flush_interval: Duration) -> Self {
        Self {
            rx,
            tasks: Default::default(),
            flush_interval,
        }
    }

    pub(crate) async fn run(mut self) {
        let mut flush = tokio::time::interval(self.flush_interval); 

        loop {
            let _ = flush.tick().await;

            while let Some(event) = self.rx.recv().now_or_never() {
                match event {
                    Some(event) => self.update(event),
                    None => {
                        tracing::debug!("Event channel closed, terminating collector");
                        return;
                    }
                };
            }

            self.send_and_flush();
        }
    }

    fn update(&mut self, event: Event) {
        match event {
            Event::Spawn { 
                id, 
                at, 
                fields,
            } => {
                let mut entry = &mut self.tasks.entry(id.clone()).or_default();
                entry.fields = fields;
                entry.stats.created_at = Some(at);

                tracing::warn!("Spawned span");
            }
            Event::Enter {
                id,
                at,
            } => {
                let mut stats = &mut self.tasks.get_mut(&id).unwrap().stats;

                if stats.current_polls == 0 {
                    stats.last_poll = Some(at);
                    if stats.first_poll == None {
                        stats.first_poll = Some(at);
                    }
                    stats.polls += 1;
                }
                stats.current_polls += 1;
                
                tracing::warn!("Entered span");
            }
            Event::Exit {
                id,
                at,
            } => {
                let mut stats = &mut self.tasks.get_mut(&id).unwrap().stats;

                stats.current_polls -= 1;
                if stats.current_polls == 0 {
                    if let Some(last_poll) = stats.last_poll {
                        stats.busy_time += at.duration_since(last_poll).unwrap();
                    }
                }

                tracing::warn!("Exited span");
            }
            Event::Close {
                id,
                at,
            } => {
                let mut stats = &mut self.tasks.get_mut(&id).unwrap().stats;
                stats.closed_at = Some(at);
                tracing::warn!("Closed span");
            }
        }
    }

    fn send_and_flush(&self) {
        for task in self.tasks.iter() {
            tracing::info!("{:?}", task);
        }
    }
}