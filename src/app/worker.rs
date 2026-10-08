//! The engine worker: owns `Engine` on one dedicated thread so the window
//! never blocks. The GUI submits a `Job` and receives a stream of `Event`s.
use iced::futures::channel::mpsc as stream;
use iced::futures::Stream;
use secblitz::engine::recover::{JournalDamaged, NotDamaged};
use secblitz::engine::{Engine, ItemChoice, Progress, Report};
use std::sync::{mpsc, Arc};

pub trait Session {
    fn available(&self) -> Vec<String>;
    fn restart_ids(&self) -> Vec<String>;
    fn audit(&mut self, progress: &mut dyn FnMut(Progress<'_>)) -> anyhow::Result<Report>;
    fn apply(
        &mut self,
        ids: &[String],
        progress: &mut dyn FnMut(Progress<'_>),
    ) -> anyhow::Result<Report>;
    fn undo(&mut self, progress: &mut dyn FnMut(Progress<'_>)) -> anyhow::Result<Report>;
    fn undo_selected(
        &mut self,
        ids: &[String],
        progress: &mut dyn FnMut(Progress<'_>),
    ) -> anyhow::Result<Report>;
    fn history(&mut self) -> anyhow::Result<Vec<String>>;
    fn can_start(&mut self, undo: bool) -> anyhow::Result<()>;
    /// The items the person picked for the controls that ask; an empty choice clears them.
    fn choose_items(&mut self, _picked: ItemChoice) -> anyhow::Result<()> {
        Ok(())
    }
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
    fn audit(&mut self, progress: &mut dyn FnMut(Progress<'_>)) -> anyhow::Result<Report> {
        self.audit_with_progress(progress)
    }
    fn apply(
        &mut self,
        ids: &[String],
        progress: &mut dyn FnMut(Progress<'_>),
    ) -> anyhow::Result<Report> {
        self.apply_selected(ids, progress)
    }
    fn undo(&mut self, progress: &mut dyn FnMut(Progress<'_>)) -> anyhow::Result<Report> {
        self.revert(progress)
    }
    fn undo_selected(
        &mut self,
        ids: &[String],
        progress: &mut dyn FnMut(Progress<'_>),
    ) -> anyhow::Result<Report> {
        self.revert_selected(ids, progress)
    }
    fn history(&mut self) -> anyhow::Result<Vec<String>> {
        Engine::history(self)
    }
    fn can_start(&mut self, undo: bool) -> anyhow::Result<()> {
        Engine::can_change(self, undo)
    }
    fn choose_items(&mut self, picked: ItemChoice) -> anyhow::Result<()> {
        Engine::choose_items(self, picked)
    }
}

type ProgressSink = Box<dyn FnMut(Progress<'_>)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Check,
    Apply(Vec<String>),
    /// A fix where the person also picked items, such as browser add-ons, by control.
    ApplyPicked {
        ids: Vec<String>,
        picked: ItemChoice,
    },
    Undo,
    UndoSome(Vec<String>),
    History,
    Preflight {
        undo: bool,
    },
    /// Move the damaged undo history aside and open a fresh one. Only answered after a failed start.
    StartFresh,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub available: Vec<String>,
    pub restart: Vec<String>,
}

pub type Outcome = Result<Arc<Report>, String>;

#[derive(Debug, Clone)]
pub enum Event {
    Opened(Result<Catalog, String>),
    /// The start failed because the saved undo history is damaged. Sent just before the failed `Opened`.
    Damaged(JournalDamaged),
    Recovered(Result<Catalog, String>),
    Progress {
        phase: Phase,
        id: String,
        status: String,
    },
    Checked(Outcome),
    Applied {
        attempted: Vec<String>,
        result: Outcome,
        verify: Outcome,
    },
    Undone {
        /// The settings asked to be put back; empty when the last fixes were undone.
        chosen: Vec<String>,
        result: Outcome,
        verify: Outcome,
    },
    History(Result<Vec<String>, String>),
    Preflight {
        undo: bool,
        result: Result<(), String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Checking,
    Applying,
    Undoing,
    Verifying,
}

type Request = (Job, stream::UnboundedSender<Event>);

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
    /// need not be `Send`. After a failed start, `recover` runs only when the
    /// window asks for `Job::StartFresh`, and `open` is tried again.
    pub fn spawn<F, R>(open: F, recover: R) -> Self
    where
        F: Fn() -> anyhow::Result<Box<dyn Session>> + Send + 'static,
        R: Fn() -> anyhow::Result<()> + Send + 'static,
    {
        let (jobs, inbox) = mpsc::channel::<Request>();
        let (opened_tx, opened_rx) = stream::unbounded();
        std::thread::Builder::new()
            .name("engine".into())
            .spawn(move || {
                let mut session = match open() {
                    Ok(session) => {
                        let _ = opened_tx.unbounded_send(Event::Opened(Ok(catalog_of(&*session))));
                        drop(opened_tx);
                        session
                    }
                    Err(error) => {
                        if let Some(damage) = error.downcast_ref::<JournalDamaged>() {
                            let _ = opened_tx.unbounded_send(Event::Damaged(*damage));
                        }
                        let _ = opened_tx.unbounded_send(Event::Opened(Err(format!("{error:#}"))));
                        drop(opened_tx);
                        match wait_for_recovery(&inbox, &open, &recover, &format!("{error:#}")) {
                            Some(session) => session,
                            None => return,
                        }
                    }
                };
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

    pub fn opened(&self) -> impl Stream<Item = Event> + Send + 'static {
        let rx = self.opened.lock().ok().and_then(|mut slot| slot.take());
        let (_tx, empty) = stream::unbounded();
        rx.unwrap_or(empty)
    }

    pub fn run(&self, job: Job) -> impl Stream<Item = Event> + Send + 'static {
        let (tx, rx) = stream::unbounded();
        if let Err(mpsc::SendError((job, tx))) = self.jobs.send((job, tx)) {
            let _ = tx.unbounded_send(failed(&job, "The engine stopped unexpectedly."));
        }
        rx
    }
}

fn catalog_of(session: &dyn Session) -> Catalog {
    Catalog {
        available: session.available(),
        restart: session.restart_ids(),
    }
}

/// Answers every job with the start-up failure until a fresh start works. `None` when the window is gone.
fn wait_for_recovery<F, R>(
    inbox: &mpsc::Receiver<Request>,
    open: &F,
    recover: &R,
    message: &str,
) -> Option<Box<dyn Session>>
where
    F: Fn() -> anyhow::Result<Box<dyn Session>>,
    R: Fn() -> anyhow::Result<()>,
{
    for (job, reply) in inbox {
        if job != Job::StartFresh {
            let _ = reply.unbounded_send(failed(&job, message));
            continue;
        }
        let recovered = match recover() {
            Err(e) if e.downcast_ref::<NotDamaged>().is_some() => Ok(()),
            other => other,
        };
        match recovered.and_then(|()| open()) {
            Ok(session) => {
                let _ = reply.unbounded_send(Event::Recovered(Ok(catalog_of(&*session))));
                return Some(session);
            }
            Err(error) => {
                eprintln!("Starting a fresh undo history failed: {error:#}");
                let _ = reply.unbounded_send(Event::Recovered(Err(format!("{error:#}"))));
            }
        }
    }
    None
}

fn failed(job: &Job, message: &str) -> Event {
    let e = || -> Outcome { Err(message.to_owned()) };
    match job {
        Job::Check => Event::Checked(e()),
        Job::Apply(ids) | Job::ApplyPicked { ids, .. } => Event::Applied {
            attempted: ids.clone(),
            result: e(),
            verify: e(),
        },
        Job::Undo => Event::Undone {
            chosen: Vec::new(),
            result: e(),
            verify: e(),
        },
        Job::UndoSome(ids) => Event::Undone {
            chosen: ids.clone(),
            result: e(),
            verify: e(),
        },
        Job::History => Event::History(Err(message.to_owned())),
        Job::Preflight { undo } => Event::Preflight {
            undo: *undo,
            result: Err(message.to_owned()),
        },
        Job::StartFresh => Event::Recovered(Err(message.to_owned())),
    }
}

fn outcome(r: anyhow::Result<Report>) -> Outcome {
    r.map(Arc::new).map_err(|e| format!("{e:#}"))
}

fn apply_in_batches(
    session: &mut dyn Session,
    ids: &[String],
    progress: &mut dyn FnMut(Progress<'_>),
) -> anyhow::Result<Report> {
    let batches = secblitz::vbs::split_batches(ids);
    if batches.len() <= 1 {
        return session.apply(ids, progress);
    }
    let mut merged = Report::default();
    for (n, batch) in batches.iter().enumerate() {
        match session.apply(batch, progress) {
            Ok(report) => {
                merged.results.extend(report.results);
                merged.findings = report.findings;
                merged.transaction = report.transaction;
                merged.readiness = report.readiness.or(merged.readiness);
            }
            Err(e) if n == 0 => return Err(e),
            Err(e) => {
                for id in batch {
                    merged.results.push(secblitz::engine::Outcome {
                        id: id.clone(),
                        title: id.clone(),
                        status: secblitz::model::CheckStatus::Error,
                        detail: format!("{e:#}"),
                        ..Default::default()
                    });
                }
                break;
            }
        }
    }
    Ok(merged)
}

fn applied(
    session: &mut dyn Session,
    ids: Vec<String>,
    picked: ItemChoice,
    progress: &dyn Fn(Phase) -> ProgressSink,
) -> Event {
    let result = match session.choose_items(picked) {
        Ok(()) => outcome(apply_in_batches(
            session,
            &ids,
            &mut *progress(Phase::Applying),
        )),
        Err(e) => Err(format!("{e:#}")),
    };
    // Checking again looks at everything, not only what was picked.
    let _ = session.choose_items(ItemChoice::new());
    let verify = outcome(session.audit(&mut *progress(Phase::Verifying)));
    Event::Applied {
        attempted: ids,
        result,
        verify,
    }
}

fn run(session: &mut dyn Session, job: Job, reply: &stream::UnboundedSender<Event>) {
    let progress = |phase: Phase| -> Box<dyn FnMut(Progress<'_>)> {
        let reply = reply.clone();
        Box::new(move |step: Progress<'_>| {
            let _ = reply.unbounded_send(Event::Progress {
                phase,
                id: step.id.to_owned(),
                status: step.step.as_str().to_owned(),
            });
        })
    };
    let event = match job {
        Job::Check => Event::Checked(outcome(session.audit(&mut *progress(Phase::Checking)))),
        Job::Apply(ids) => applied(session, ids, ItemChoice::new(), &progress),
        Job::ApplyPicked { ids, picked } => applied(session, ids, picked, &progress),
        Job::Undo => {
            let result = outcome(session.undo(&mut *progress(Phase::Undoing)));
            let verify = outcome(session.audit(&mut *progress(Phase::Verifying)));
            Event::Undone {
                chosen: Vec::new(),
                result,
                verify,
            }
        }
        Job::UndoSome(ids) => {
            let result = outcome(session.undo_selected(&ids, &mut *progress(Phase::Undoing)));
            let verify = outcome(session.audit(&mut *progress(Phase::Verifying)));
            Event::Undone {
                chosen: ids,
                result,
                verify,
            }
        }
        Job::History => Event::History(session.history().map_err(|e| format!("{e:#}"))),
        Job::Preflight { undo } => Event::Preflight {
            undo,
            result: session.can_start(undo).map_err(|e| format!("{e:#}")),
        },
        Job::StartFresh => Event::Recovered(Err("Nothing needs to be started fresh.".into())),
    };
    let _ = reply.unbounded_send(event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::futures::executor::block_on;
    use iced::futures::StreamExt;
    use secblitz::engine::ProgressStep;
    use secblitz::model::CheckStatus;

    type Log = Arc<std::sync::Mutex<Vec<String>>>;

    struct Fake {
        log: Log,
        fail_apply: bool,
        fail_audit: bool,
        picked: usize,
        audited_with: Arc<std::sync::Mutex<Vec<usize>>>,
    }

    impl Fake {
        fn note(&self, what: impl Into<String>) {
            self.log.lock().unwrap().push(what.into());
        }
    }

    fn report(tag: &str) -> Report {
        Report {
            transaction: Some(tag.to_owned()),
            ..Report::default()
        }
    }

    impl Session for Fake {
        fn available(&self) -> Vec<String> {
            vec!["a".into(), "b".into()]
        }
        fn restart_ids(&self) -> Vec<String> {
            vec!["b".into()]
        }
        fn choose_items(&mut self, picked: ItemChoice) -> anyhow::Result<()> {
            self.picked = picked.values().map(Vec::len).sum();
            if self.picked > 0 {
                self.note(format!("choose {}", self.picked));
            }
            Ok(())
        }
        fn audit(&mut self, progress: &mut dyn FnMut(Progress<'_>)) -> anyhow::Result<Report> {
            self.audited_with.lock().unwrap().push(self.picked);
            self.note("audit");
            progress(Progress::new(
                "a",
                ProgressStep::Result(CheckStatus::Compliant),
            ));
            if self.fail_audit {
                anyhow::bail!("audit broke");
            }
            Ok(report("audit"))
        }
        fn apply(
            &mut self,
            ids: &[String],
            progress: &mut dyn FnMut(Progress<'_>),
        ) -> anyhow::Result<Report> {
            self.note(format!("apply {}", ids.join(",")));
            for id in ids {
                progress(Progress::new(
                    id,
                    ProgressStep::Result(CheckStatus::Applied),
                ));
            }
            if self.fail_apply {
                anyhow::bail!("disk full");
            }
            Ok(report("apply"))
        }
        fn undo(&mut self, progress: &mut dyn FnMut(Progress<'_>)) -> anyhow::Result<Report> {
            self.note("undo");
            progress(Progress::new(
                "a",
                ProgressStep::Result(CheckStatus::Restored),
            ));
            Ok(report("undo"))
        }
        fn undo_selected(
            &mut self,
            ids: &[String],
            progress: &mut dyn FnMut(Progress<'_>),
        ) -> anyhow::Result<Report> {
            self.note(format!("undo_selected {}", ids.join(",")));
            for id in ids {
                progress(Progress::new(
                    id,
                    ProgressStep::Result(CheckStatus::Restored),
                ));
            }
            if self.fail_apply {
                anyhow::bail!("disk full");
            }
            Ok(report("undo_selected"))
        }
        fn history(&mut self) -> anyhow::Result<Vec<String>> {
            Ok(vec!["tx applied".into()])
        }
        fn can_start(&mut self, undo: bool) -> anyhow::Result<()> {
            self.note(format!("can_change {undo}"));
            if self.fail_apply && !undo {
                anyhow::bail!("Repair readiness blocks new changes");
            }
            Ok(())
        }
    }

    fn worker(fail_apply: bool, fail_audit: bool) -> (Worker, Log) {
        let (w, log, _) = worker_seeing(fail_apply, fail_audit);
        (w, log)
    }

    fn worker_seeing(
        fail_apply: bool,
        fail_audit: bool,
    ) -> (Worker, Log, Arc<std::sync::Mutex<Vec<usize>>>) {
        let log = Log::default();
        let l = log.clone();
        let audited_with = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = audited_with.clone();
        let w = Worker::spawn(
            move || {
                Ok(Box::new(Fake {
                    log: l.clone(),
                    fail_apply,
                    fail_audit,
                    picked: 0,
                    audited_with: seen.clone(),
                }) as Box<dyn Session>)
            },
            || Ok(()),
        );
        (w, log, audited_with)
    }

    fn collect(w: &Worker, job: Job) -> Vec<Event> {
        block_on(w.run(job).collect())
    }

    #[test]
    fn picked_items_reach_the_fix_and_are_cleared_before_the_check_that_follows() {
        let (w, log, audited_with) = worker_seeing(false, false);
        let picked = ItemChoice::from([("x".to_owned(), vec!["i".to_owned(), "j".to_owned()])]);
        let events = collect(
            &w,
            Job::ApplyPicked {
                ids: vec!["x".into()],
                picked,
            },
        );
        assert_eq!(*log.lock().unwrap(), ["choose 2", "apply x", "audit"]);
        assert_eq!(*audited_with.lock().unwrap(), [0]);
        assert!(matches!(
            events.last(),
            Some(Event::Applied { attempted, result: Ok(_), verify: Ok(_) }) if attempted == &["x"]
        ));
    }

    #[test]
    fn a_plain_fix_picks_nothing() {
        let (w, log) = worker(false, false);
        collect(&w, Job::Apply(vec!["x".into()]));
        assert_eq!(*log.lock().unwrap(), ["apply x", "audit"]);
    }

    #[test]
    fn opened_reports_catalog_once() {
        let (w, _) = worker(false, false);
        let first: Vec<Event> = block_on(w.opened().collect());
        match first.as_slice() {
            [Event::Opened(Ok(c))] => {
                assert_eq!(c.available, vec!["a", "b"]);
                assert_eq!(c.restart, vec!["b"]);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(block_on(w.opened().collect::<Vec<_>>()).is_empty());
    }

    #[test]
    fn check_forwards_progress_then_result() {
        let (w, _) = worker(false, false);
        let events = collect(&w, Job::Check);
        assert!(matches!(
            &events[0],
            Event::Progress { phase: Phase::Checking, id, status } if id == "a" && status == "compliant"
        ));
        assert!(matches!(events.last(), Some(Event::Checked(Ok(_)))));
    }

    #[test]
    fn each_core_protection_is_applied_as_its_own_batch() {
        let (w, log) = worker(false, false);
        let ids = vec![
            "a".to_owned(),
            secblitz::vbs::MEMORY_INTEGRITY.to_owned(),
            "b".to_owned(),
            secblitz::vbs::STACK_PROTECTION.to_owned(),
        ];
        let events = collect(&w, Job::Apply(ids.clone()));
        assert_eq!(
            *log.lock().unwrap(),
            vec![
                "apply a,b".to_owned(),
                format!("apply {}", secblitz::vbs::MEMORY_INTEGRITY),
                format!("apply {}", secblitz::vbs::STACK_PROTECTION),
                "audit".to_owned(),
            ]
        );
        assert!(matches!(
            events.last(),
            Some(Event::Applied { result: Ok(_), .. })
        ));
        let (w, log) = worker(false, false);
        collect(&w, Job::Apply(vec!["a".into(), "b".into()]));
        assert_eq!(*log.lock().unwrap(), vec!["apply a,b", "audit"]);
    }

    #[test]
    fn apply_is_always_followed_by_verify_even_when_apply_fails() {
        let (w, log) = worker(true, false);
        let events = collect(&w, Job::Apply(vec!["a".into(), "b".into()]));
        assert_eq!(*log.lock().unwrap(), vec!["apply a,b", "audit"]);
        let phases: Vec<Phase> = events
            .iter()
            .filter_map(|e| match e {
                Event::Progress { phase, .. } => Some(*phase),
                _ => None,
            })
            .collect();
        assert_eq!(
            phases,
            vec![Phase::Applying, Phase::Applying, Phase::Verifying]
        );
        match events.last() {
            Some(Event::Applied {
                attempted,
                result,
                verify,
            }) => {
                assert_eq!(attempted, &vec!["a".to_owned(), "b".to_owned()]);
                assert!(result.as_ref().unwrap_err().contains("disk full"));
                assert!(verify.is_ok());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn failed_verify_is_reported_separately() {
        let (w, _) = worker(false, true);
        match collect(&w, Job::Apply(vec!["a".into()])).last() {
            Some(Event::Applied { result, verify, .. }) => {
                assert!(result.is_ok());
                assert!(verify.as_ref().unwrap_err().contains("audit broke"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn undo_is_followed_by_verify() {
        let (w, log) = worker(false, false);
        let events = collect(&w, Job::Undo);
        assert_eq!(*log.lock().unwrap(), vec!["undo", "audit"]);
        assert!(matches!(
            events.last(),
            Some(Event::Undone {
                chosen,
                result: Ok(_),
                verify: Ok(_)
            }) if chosen.is_empty()
        ));
    }

    #[test]
    fn chosen_settings_are_put_back_then_verified() {
        let (w, log) = worker(false, false);
        let events = collect(&w, Job::UndoSome(vec!["a".into(), "b".into()]));
        assert_eq!(*log.lock().unwrap(), vec!["undo_selected a,b", "audit"]);
        let phases: Vec<Phase> = events
            .iter()
            .filter_map(|e| match e {
                Event::Progress { phase, .. } => Some(*phase),
                _ => None,
            })
            .collect();
        assert_eq!(
            phases,
            vec![Phase::Undoing, Phase::Undoing, Phase::Verifying]
        );
        match events.last() {
            Some(Event::Undone {
                chosen,
                result: Ok(_),
                verify: Ok(_),
            }) => assert_eq!(chosen, &vec!["a".to_owned(), "b".to_owned()]),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_failed_chosen_undo_is_still_verified() {
        let (w, log) = worker(true, false);
        match collect(&w, Job::UndoSome(vec!["a".into()])).last() {
            Some(Event::Undone {
                chosen,
                result,
                verify,
            }) => {
                assert_eq!(chosen, &vec!["a".to_owned()]);
                assert!(result.as_ref().unwrap_err().contains("disk full"));
                assert!(verify.is_ok());
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(*log.lock().unwrap(), vec!["undo_selected a", "audit"]);
    }

    #[test]
    fn preflight_answers_without_applying_or_checking() {
        let (w, log) = worker(false, false);
        match collect(&w, Job::Preflight { undo: false }).last() {
            Some(Event::Preflight {
                undo: false,
                result: Ok(()),
            }) => {}
            other => panic!("unexpected {other:?}"),
        }
        let (w, log2) = worker(true, false);
        match collect(&w, Job::Preflight { undo: false }).last() {
            Some(Event::Preflight { result: Err(e), .. }) => assert!(e.contains("readiness")),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(*log.lock().unwrap(), vec!["can_change false"]);
        assert_eq!(*log2.lock().unwrap(), vec!["can_change false"]);
    }

    #[test]
    fn history_job_returns_lines() {
        let (w, _) = worker(false, false);
        match collect(&w, Job::History).last() {
            Some(Event::History(Ok(lines))) => assert_eq!(lines, &vec!["tx applied".to_owned()]),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn open_failure_answers_every_job_with_the_error() {
        let w = Worker::spawn(|| anyhow::bail!("cannot open"), || Ok(()));
        match block_on(w.opened().collect::<Vec<_>>()).as_slice() {
            [Event::Opened(Err(e))] => assert!(e.contains("cannot open")),
            other => panic!("unexpected {other:?}"),
        }
        for job in [
            Job::Check,
            Job::Apply(vec!["a".into()]),
            Job::Undo,
            Job::UndoSome(vec!["a".into()]),
            Job::History,
            Job::Preflight { undo: true },
        ] {
            let events = collect(&w, job.clone());
            assert_eq!(events.len(), 1, "{job:?}");
            let text = format!("{:?}", events[0]);
            assert!(text.contains("cannot open"), "{text}");
        }
    }

    fn damaged_worker(recovers: bool) -> Worker {
        use std::sync::atomic::{AtomicBool, Ordering};
        let fresh = Arc::new(AtomicBool::new(false));
        let flag = fresh.clone();
        Worker::spawn(
            move || {
                if flag.load(Ordering::SeqCst) {
                    Ok(Box::new(Fake {
                        log: Log::default(),
                        fail_apply: false,
                        fail_audit: false,
                        picked: 0,
                        audited_with: Arc::default(),
                    }) as Box<dyn Session>)
                } else {
                    Err(anyhow::Error::new(JournalDamaged {
                        kind: secblitz::engine::recover::DamageKind::Total,
                        files: 2,
                    })
                    .context("outer"))
                }
            },
            move || {
                anyhow::ensure!(recovers, "files are in use");
                fresh.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
    }

    #[test]
    fn damaged_history_is_reported_before_the_failed_start() {
        let w = damaged_worker(true);
        match block_on(w.opened().collect::<Vec<_>>()).as_slice() {
            [Event::Damaged(d), Event::Opened(Err(_))] => assert_eq!(d.files, 2),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn starting_fresh_reopens_the_session_and_jobs_work_again() {
        let w = damaged_worker(true);
        block_on(w.opened().collect::<Vec<_>>());
        assert!(matches!(
            collect(&w, Job::Check).as_slice(),
            [Event::Checked(Err(_))]
        ));
        match collect(&w, Job::StartFresh).as_slice() {
            [Event::Recovered(Ok(c))] => assert_eq!(c.available, vec!["a", "b"]),
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            collect(&w, Job::Check).last(),
            Some(Event::Checked(Ok(_)))
        ));
    }

    #[test]
    fn a_failed_fresh_start_leaves_the_session_closed_and_can_be_retried() {
        let w = damaged_worker(false);
        block_on(w.opened().collect::<Vec<_>>());
        match collect(&w, Job::StartFresh).as_slice() {
            [Event::Recovered(Err(e))] => assert!(e.contains("files are in use")),
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            collect(&w, Job::Check).as_slice(),
            [Event::Checked(Err(_))]
        ));
    }

    #[test]
    fn starting_fresh_does_nothing_when_the_start_worked() {
        let (w, _) = worker(false, false);
        assert!(matches!(
            collect(&w, Job::StartFresh).as_slice(),
            [Event::Recovered(Err(_))]
        ));
    }
}
