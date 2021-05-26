use crate::{collector::Collector, event::Event};

use tokio::sync::mpsc::{self, error::TrySendError, Sender};
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

use std::time::{Duration, SystemTime};

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
                    Ok(_) => extensions.insert(FormattedFields::<F>::new(fields.clone())),
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