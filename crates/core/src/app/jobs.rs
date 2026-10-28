//! Arka plan işleri: ağ istekleri gibi uzun işler ayrı iş parçacığında
//! yürür, okuma etkileşimi beklemez. Sonuç `Event::Tick` geldikçe yoklanır.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

pub(super) struct Job<T> {
    rx: Receiver<T>,
    cancel: Arc<AtomicBool>,
}

impl<T: Send + 'static> Job<T> {
    /// `work` ayrı iş parçacığında çalışır; iptal bayrağına bakabilir.
    pub fn spawn(work: impl FnOnce(&AtomicBool) -> T + Send + 'static) -> Job<T> {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work(&flag));
        });
        Job { rx, cancel }
    }

    /// Bittiyse sonuç; iş parçacığı sonuç vermeden öldüyse `Some(None)`.
    pub fn poll(&self) -> Option<Option<T>> {
        match self.rx.try_recv() {
            Ok(v) => Some(Some(v)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(None),
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Testlerde: sonuç gelene kadar bekler.
    #[cfg(test)]
    pub fn wait(&self) -> Option<T> {
        self.rx.recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_and_cancellation() {
        let job = Job::spawn(|_| 42);
        assert_eq!(job.wait(), Some(42));
        let job = Job::spawn(|cancel: &AtomicBool| {
            while !cancel.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            "iptal"
        });
        assert!(job.poll().is_none(), "henüz bitmedi");
        job.cancel();
        assert_eq!(job.wait(), Some("iptal"));
    }
}
