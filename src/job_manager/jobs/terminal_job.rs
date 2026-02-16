use crate::document::DocumentId;
use crate::job_manager::{CancellationSignal, Job, JobMessage};
use crate::term::TerminalEvent;
use std::fmt;
use std::sync::mpsc::{Receiver, Sender};

pub struct TerminalInputJob {
    pub document_id: DocumentId,
    pub rx: Receiver<TerminalEvent>,
}

impl fmt::Debug for TerminalInputJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TerminalInputJob")
            .field("document_id", &self.document_id)
            .finish()
    }
}

impl Job for TerminalInputJob {
    fn run(self: Box<Self>, _id: usize, tx: Sender<JobMessage>, signal: CancellationSignal) {
        loop {
            if signal.is_cancelled() {
                break;
            }

            match self.rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(event) => match event {
                    TerminalEvent::Wakeup => {
                        while let Ok(TerminalEvent::Wakeup) = self.rx.try_recv() {}
                        if tx
                            .send(JobMessage::TerminalOutput(self.document_id, vec![]))
                            .is_err()
                        {
                            break;
                        }
                    }
                    TerminalEvent::ChildExit(_code) => {
                        let _ = tx.send(JobMessage::TerminalExit(self.document_id));
                        break;
                    }
                    TerminalEvent::Title(_) => {}
                },
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = tx.send(JobMessage::TerminalExit(self.document_id));
                    break;
                }
            }
        }
    }

    fn is_silent(&self) -> bool {
        true
    }
}
