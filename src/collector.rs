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
    active: bool,
    current_polls: u64,
    created_at: Option<SystemTime>,
    first_poll: Option<SystemTime>,
    last_poll: Option<SystemTime>,
    closed_at: Option<SystemTime>,
    busy_time: Duration,
}

impl Stats {
    pub fn total_time(&self) -> Option<Duration> {
        self.closed_at.and_then(|end| {
            self.created_at.and_then(|start| {
                end.duration_since(start).ok()
            })
        })
    }
}

pub struct Collector {
    events: Receiver<Event>,
    tasks: HashMap<Id, Task>,
    tick_interval: Duration,
}

impl Collector {
    pub fn new(events: Receiver<Event>, tick_interval: Duration) -> Self {
        Self {
            events,
            tasks: Default::default(),
            tick_interval,
        }
    }

    pub async fn run(mut self) {
        let mut flush = tokio::time::interval(self.tick_interval); 

        loop {
            let _ = flush.tick().await;

            while let Some(event) = self.events.recv().now_or_never() {
                match event {
                    Some(event) => self.update(event),
                    None => {
                        tracing::debug!("Event channel closed, terminating collector");
                        return;
                    }
                };
            }

            self.produce_metrics();
        }
    }

    fn update(&mut self, event: Event) {
        match event {
            Event::Spawn { 
                id, 
                time, 
                fields,
            } => {
                let mut entry = &mut self.tasks.entry(id.clone()).or_default();
                entry.fields = fields;
                entry.stats.created_at = Some(time);
                entry.stats.active = true;
            }
            Event::Enter { id, time } => {
                let mut stats = &mut self.tasks.get_mut(&id).unwrap().stats;

                if stats.current_polls == 0 {
                    stats.last_poll = Some(time);
                    if stats.first_poll == None {
                        stats.first_poll = Some(time);
                    }
                }

                stats.current_polls += 1;
            }
            Event::Exit { id, time } => {
                let mut stats = &mut self.tasks.get_mut(&id).unwrap().stats;
                stats.current_polls -= 1;

                if stats.current_polls == 0 {
                    if let Some(last_poll) = stats.last_poll {
                        stats.busy_time += time.duration_since(last_poll).unwrap();
                    }
                }
            }
            Event::Close { id, time } => {
                let mut stats = &mut self.tasks.get_mut(&id).unwrap().stats;
                stats.active = false;
                stats.closed_at = Some(time);
            }
        }
    }

    fn produce_metrics(&self) {
        for task in self.tasks.iter() {
            if task.1.stats.active {
                tracing::info!("Task {} running", task.0.into_u64());
            } else {
                tracing::info!("Task {} inactive: total time {:?}", task.0.into_u64(), task.1.stats.total_time());
            }
        }
    }
}