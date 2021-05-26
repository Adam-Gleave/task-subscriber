use futures::FutureExt;
use tokio::sync::mpsc::{self, error::TrySendError, Receiver, Sender};
use tracing_core::{
    span::{self, Id},
    subscriber::Subscriber,
};
use tracing_subscriber::{
    Layer,
    fmt::{
        format::{DefaultFields, FormatFields}, 
        FormattedFields,
    },
    layer::Context,
    registry::LookupSpan,
};

use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

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

pub struct BeeLayer<F = DefaultFields> {
    tx: Sender<Event>,
    format: F,
    collector: Option<Collector>,
}

impl BeeLayer {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(100);

        Self {
            tx: tx,
            format: Default::default(),
            collector: Some(Collector::new(rx, Duration::from_secs(Self::FLUSH_INTERVAL))),
        }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let collector = self
            .collector
            .expect("No collector");

        let collector = tokio::spawn(async move { collector.run().await });
        let res = collector.await;
        res.map_err(Into::into)
    }
}

impl<F> BeeLayer<F> {
    const FLUSH_INTERVAL: u64 = 1;

    fn send(&self, event: Event) {
        match self.tx.try_reserve() {
            Ok(permit) => permit.send(event),
            Err(TrySendError::Closed(_)) => tracing::error!("Receiver terminated"),
            _ => tracing::error!("Unknown error"),
        }
    }
}

impl<S, F> Layer<S> for BeeLayer<F> 
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    F: for<'writer> FormatFields<'writer> + 'static,
{
    fn new_span(&self, attrs: &span::Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span does not exist");
        let mut extensions = span.extensions_mut();

        let fields = match extensions.get_mut::<FormattedFields<F>>() {
            Some(fields) => fields.fields.clone(),
            None => {
                let mut fields = String::new();

                match self.format.format_fields(&mut fields, attrs) {
                    Ok(()) => extensions.insert(FormattedFields::<F>::new(fields.clone())),
                    Err(_) => {
                        tracing::warn!("Error formatting span fields");
                    },
                }
                fields
            }
        };

        self.send(Event::Spawn {
            id: id.clone(),
            at: SystemTime::now(),
            fields,
        });
    }

    fn on_enter(&self, id: &Id, _ctx: Context<'_, S>) {
        self.send(Event::Enter {
            at: SystemTime::now(),
            id: id.clone(),
        });
    }

    fn on_exit(&self, id: &Id, _ctx: Context<'_, S>) {
        self.send(Event::Exit {
            at: SystemTime::now(),
            id: id.clone(),
        });
    }

    fn on_close(&self, id: Id, _ctx: Context<'_, S>) {
        self.send(Event::Close {
            at: SystemTime::now(),
            id: id.clone(),
        });
    }
}

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

struct Collector {
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
