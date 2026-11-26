//! Running archive work off the window's thread. A job reports progress through its `Ctl`;
//! when it needs an answer (replace a file? which password?) it waits on a channel until the
//! window answers.

use archivecraft_engine::model::{Answer, Ctl, Question};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    Create,
    Extract,
    Test,
    Delete,
    Convert,
    Repair,
    Comment,
    Find,
    Bench,
    Recovery,
}

pub struct Job<T: Send + 'static> {
    pub kind: JobKind,
    pub title: String,
    pub ctl: Arc<Ctl>,
    handle: Option<JoinHandle<T>>,
    questions: Receiver<(Question, Sender<Answer>)>,
    /// The question on screen now, with where to send the answer.
    pub asking: Option<(Question, Sender<Answer>)>,
    /// Hidden behind the status bar instead of in its own window.
    pub background: bool,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(kind: JobKind, title: String, ctx: egui::Context, work: impl FnOnce(&Ctl) -> T + Send + 'static) -> Job<T> {
        let (tx, rx) = channel::<(Question, Sender<Answer>)>();
        let wake = ctx.clone();
        let ctl = Ctl::with_asker(Box::new(move |q| {
            let (atx, arx) = channel();
            if tx.send((q, atx)).is_err() {
                return Answer::Cancel;
            }
            wake.request_repaint();
            arx.recv().unwrap_or(Answer::Cancel)
        }));
        let c = ctl.clone();
        let handle = std::thread::Builder::new()
            .name(format!("archivecraft-{kind:?}"))
            .spawn(move || {
                let out = work(&c);
                ctx.request_repaint();
                out
            })
            .expect("a worker thread can start");
        Job { kind, title, ctl, handle: Some(handle), questions: rx, asking: None, background: false }
    }

    /// Pick up a waiting question, if any.
    pub fn poll_question(&mut self) {
        if self.asking.is_none()
            && let Ok(q) = self.questions.try_recv()
        {
            self.asking = Some(q);
        }
    }

    pub fn answer(&mut self, a: Answer) {
        if let Some((_, tx)) = self.asking.take() {
            let _ = tx.send(a);
        }
    }

    pub fn is_done(&self) -> bool {
        self.handle.as_ref().is_none_or(|h| h.is_finished())
    }

    /// The result, once finished. `None` while running or if the work panicked.
    pub fn take(&mut self) -> Option<T> {
        if !self.is_done() {
            return None;
        }
        self.handle.take().and_then(|h| h.join().ok())
    }
}
