//! Running blocking work and timers off the interface thread.
use iced::futures::channel::{mpsc, oneshot};
use iced::futures::{Future, Stream};
use iced::Subscription;

pub fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = T> + Send + 'static {
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    // A panicked closure drops the sender: keep the UI alive rather than
    // panicking inside the executor (the page's own error handling applies).
    async move {
        match rx.await {
            Ok(value) => value,
            Err(_) => std::future::pending().await,
        }
    }
}

pub fn blocking_stream<T: Send + 'static>(
    f: impl FnOnce(&dyn Fn(T)) + Send + 'static,
) -> impl Stream<Item = T> + Send + 'static {
    let (tx, rx) = mpsc::unbounded();
    std::thread::spawn(move || {
        let emit = move |item: T| {
            let _ = tx.unbounded_send(item);
        };
        f(&emit);
    });
    rx
}

fn ticker(period: std::time::Duration) -> impl Stream<Item = std::time::Instant> + Send + 'static {
    let (tx, rx) = mpsc::unbounded();
    std::thread::spawn(move || loop {
        std::thread::sleep(period);
        if tx.unbounded_send(std::time::Instant::now()).is_err() {
            break;
        }
    });
    rx
}

pub fn ticks_100ms() -> Subscription<std::time::Instant> {
    Subscription::run(|| ticker(std::time::Duration::from_millis(100)))
}
