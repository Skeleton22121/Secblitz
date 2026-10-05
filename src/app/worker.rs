//! The engine worker: owns `Engine` on one dedicated thread so the window
//! never blocks. The GUI submits a `Job` and receives a stream of `Event`s.
//!
//! OWNER: app-core agent (implementation details may change; the public
//! `Job`/`Event`/`Worker` surface is the contract used by the GUI).
use iced::futures::channel::mpsc as stream;
use iced::futures::Stream;
use secblitz::engine::{Engine, Report};
use std::sync::{mpsc, Arc};

/// Everything the GUI needs from the hardening engine.
pub trait Session {
    fn available(&self) -> Vec<String>;
    fn restart_ids(&self) -> Vec<String>;
    fn audit(&mut self, progress: &mut dyn FnMut(&str, &str)) -> anyhow::Result<Report>;
    fn apply(
        &mut self,
        ids: &[String],
        progress: &mut dyn FnMut(&str, &str),
    ) -> anyhow::Result<Report>;
    fn undo(&mut self, progress: &mut dyn FnMut(&str, &str)) -> anyhow::Result<Report>;
    fn history(&mut self) -> anyhow::Result<Vec<String>>;
}

impl Session for Engine {
    fn available(&self) -> Vec<String> {
        self.available_controls()
            .iter()
            .map(|c| c.id.clone())
            .collect()
    }
    fn restart_ids(&self) -> Vec<String> {
        self.available_controls()
            .iter()
            .filter(|c| c.reboot)
            .map(|c| c.id.clone())
            .collect()
    }
    fn audit(&mut self, progress: &mut dyn FnMut(&str, &str)) -> anyhow::Result<Report> {
        self.audit_with_progress(progress)
    }
    fn apply(
        &mut self,
        ids: &[String],
        progress: &mut dyn FnMut(&str, &str),
    ) -> anyhow::Result<Report> {
        self.apply_selected(ids, progress)
    }
    fn undo(&mut self, progress: &mut dyn FnMut(&str, &str)) -> anyhow::Result<Report> {
        self.revert(progress)
    }
    fn history(&mut self) -> anyhow::Result<Vec<String>> {
        Engine::history(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    /// Read-only protection check.
    Check,
    /// Apply exactly these fixes, then always run a fresh post-check.
    Apply(Vec<String>),
    /// Undo the newest recorded batch, then always run a fresh post-check.
    Undo,
    /// Read the engine's batch history (newest first).
    History,
}

/// What the engine knows about its catalog. Sent once after opening.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub available: Vec<String>,
    pub restart: Vec<String>,
}

/// Errors cross the channel as display strings: `anyhow::Error` is not Clone.
pub type Outcome = Result<Arc<Report>, String>;

#[derive(Debug, Clone)]
pub enum Event {
    /// The engine opened (or failed to). Always the first event ever sent.
    Opened(Result<Catalog, String>),
    /// Live progress: (item id or phase, status word).
    Progress { phase: Phase, id: String, status: String },
    Checked(Outcome),
    Applied {
        attempted: Vec<String>,
        result: Outcome,
        verify: Outcome,
    },
    Undone {
        result: Outcome,
        verify: Outcome,
    },
    History(Result<Vec<String>, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Checking,
    Applying,
    Undoing,
    Verifying,
}

type Request = (Job, stream::UnboundedSender<Event>);

/// Cheap to clone handle to the engine thread.
#[derive(Clone)]
pub struct Worker {
    jobs: mpsc::Sender<Request>,
    opened: Arc<std::sync::Mutex<Option<stream::UnboundedReceiver<Event>>>>,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Worker")
    }
}

impl Worker {
    /// Start the engine thread. `open` runs on that thread, so the session
    /// need not be `Send`.
    pub fn spawn<F>(open: F) -> Self
    where
        F: FnOnce() -> anyhow::Result<Box<dyn Session>> + Send + 'static,
    {
        let (jobs, inbox) = mpsc::channel::<Request>();
        let (opened_tx, opened_rx) = stream::unbounded();
        std::thread::Builder::new()
            .name("engine".into())
            .spawn(move || {
                let mut session = match open() {
                    Ok(session) => {
                        let _ = opened_tx.unbounded_send(Event::Opened(Ok(Catalog {
                            available: session.available(),
                            restart: session.restart_ids(),
                        })));
                        session
                    }
                    Err(error) => {
                        let _ = opened_tx.unbounded_send(Event::Opened(Err(format!("{error:#}"))));
                        // Keep answering jobs with the open failure.
                        let message = format!("{error:#}");
                        for (job, reply) in inbox {
                            let _ = reply.unbounded_send(failed(&job, &message));
                        }
                        return;
                    }
                };
                drop(opened_tx);
                for (job, reply) in inbox {
                    run(session.as_mut(), job, &reply);
                }
            })
            .expect("spawn engine thread");
        Self {
            jobs,
            opened: Arc::new(std::sync::Mutex::new(Some(opened_rx))),
        }
    }

    /// The one-shot `Opened` event stream. Returns an empty stream if taken.
    pub fn opened(&self) -> impl Stream<Item = Event> + Send + 'static {
        let rx = self.opened.lock().ok().and_then(|mut slot| slot.take());
        let (_tx, empty) = stream::unbounded();
        rx.unwrap_or(empty)
    }

    /// Submit a job; the returned stream ends after the job's final event.
    pub fn run(&self, job: Job) -> impl Stream<Item = Event> + Send + 'static {
        let (tx, rx) = stream::unbounded();
        if let Err(mpsc::SendError((job, tx))) = self.jobs.send((job, tx)) {
            let _ = tx.unbounded_send(failed(&job, "The engine stopped unexpectedly."));
        }
        rx
    }
}

fn failed(job: &Job, message: &str) -> Event {
    let e = || -> Outcome { Err(message.to_owned()) };
    match job {
        Job::Check => Event::Checked(e()),
        Job::Apply(ids) => Event::Applied {
            attempted: ids.clone(),
            result: e(),
            verify: e(),
        },
        Job::Undo => Event::Undone {
            result: e(),
            verify: e(),
        },
        Job::History => Event::History(Err(message.to_owned())),
    }
}

fn outcome(r: anyhow::Result<Report>) -> Outcome {
    r.map(Arc::new).map_err(|e| format!("{e:#}"))
}

fn run(session: &mut dyn Session, job: Job, reply: &stream::UnboundedSender<Event>) {
    let progress = |phase: Phase| {
        let reply = reply.clone();
        move |id: &str, status: &str| {
            let _ = reply.unbounded_send(Event::Progress {
                phase,
                id: id.to_owned(),
                status: status.to_owned(),
            });
        }
    };
    let event = match job {
        Job::Check => Event::Checked(outcome(session.audit(&mut progress(Phase::Checking)))),
        Job::Apply(ids) => {
            let result = outcome(session.apply(&ids, &mut progress(Phase::Applying)));
            // Always verify, even after a failed or partial apply.
            let verify = outcome(session.audit(&mut progress(Phase::Verifying)));
            Event::Applied {
                attempted: ids,
                result,
                verify,
            }
        }
        Job::Undo => {
            let result = outcome(session.undo(&mut progress(Phase::Undoing)));
            let verify = outcome(session.audit(&mut progress(Phase::Verifying)));
            Event::Undone { result, verify }
        }
        Job::History => Event::History(session.history().map_err(|e| format!("{e:#}"))),
    };
    let _ = reply.unbounded_send(event);
}
