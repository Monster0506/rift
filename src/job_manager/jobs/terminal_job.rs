use crate::document::DocumentId;
use crate::job_manager::{
    AsyncToken, CancellationSignal, Job, JobMessage, JobPayload, JobRef, JobTarget,
};
use crate::term::TerminalEvent;
use std::fmt;
use std::sync::mpsc::{Receiver, Sender};

#[derive(Debug)]
pub(crate) struct TerminalOutput;

impl JobPayload for TerminalOutput {
    fn apply(self: Box<Self>, editor: &mut dyn JobTarget, job: JobRef) {
        if let Some(token) = job.token {
            editor.apply_terminal_output(token);
        }
    }

    fn is_terminal_output(&self) -> bool {
        true
    }
}

#[derive(Debug)]
pub(crate) struct TerminalExit;

impl JobPayload for TerminalExit {
    fn apply(self: Box<Self>, editor: &mut dyn JobTarget, job: JobRef) {
        if let Some(token) = job.token {
            editor.apply_terminal_exit(token);
        }
    }
}

pub(crate) struct TerminalInputJob {
    document_id: DocumentId,
    rx: Receiver<TerminalEvent>,
    token: AsyncToken,
}

impl TerminalInputJob {
    #[must_use]
    pub(crate) const fn new(token: AsyncToken, rx: Receiver<TerminalEvent>) -> Self {
        Self {
            document_id: token.doc_id(),
            rx,
            token,
        }
    }
}

impl fmt::Debug for TerminalInputJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TerminalInputJob")
            .field("document_id", &self.document_id)
            .field("token", &self.token)
            .finish_non_exhaustive()
    }
}

impl Job for TerminalInputJob {
    fn name(&self) -> &'static str {
        "terminal"
    }

    fn async_token(&self) -> Option<AsyncToken> {
        Some(self.token)
    }

    fn is_long_lived(&self) -> bool {
        true
    }

    fn run(self: Box<Self>, id: usize, tx: Sender<JobMessage>, signal: CancellationSignal) {
        let mut pending = None;
        loop {
            if signal.is_cancelled() {
                break;
            }

            let event = pending.take().map_or_else(
                || self.rx.recv_timeout(std::time::Duration::from_millis(50)),
                Ok,
            );
            match event {
                Ok(event) => match event {
                    TerminalEvent::Wakeup => {
                        for _ in 0..255 {
                            match self.rx.try_recv() {
                                Ok(TerminalEvent::Wakeup | TerminalEvent::Title) => {}
                                Ok(exit @ TerminalEvent::ChildExit(_)) => {
                                    pending = Some(exit);
                                    break;
                                }
                                Err(_) => break,
                            }
                        }
                        if tx
                            .send(JobMessage::CustomToken(
                                id,
                                self.token,
                                Box::new(TerminalOutput),
                            ))
                            .is_err()
                        {
                            break;
                        }
                    }
                    TerminalEvent::ChildExit(_code) => {
                        let _ = tx.send(JobMessage::CustomToken(
                            id,
                            self.token,
                            Box::new(TerminalExit),
                        ));
                        break;
                    }
                    TerminalEvent::Title => {}
                },
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = tx.send(JobMessage::CustomToken(
                        id,
                        self.token,
                        Box::new(TerminalExit),
                    ));
                    break;
                }
            }
        }
        if !signal.is_cancelled() {
            let _ = tx.send(JobMessage::Finished(id, true));
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}
