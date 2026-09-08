use crate::deadline::Deadline;
use crate::error::{Error, ErrorKind};
use std::future::Future;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct Operation {
    deadline: Deadline,
    cancellation: CancellationToken,
}

impl Operation {
    pub fn new(deadline: Deadline, cancellation: CancellationToken) -> Self {
        Self {
            deadline,
            cancellation,
        }
    }

    pub fn remaining(&self) -> Result<Duration, Error> {
        if self.cancellation.is_cancelled() {
            return Err(cancelled());
        }
        self.deadline.remaining()
    }

    pub(crate) async fn run<T>(
        &self,
        future: impl Future<Output = Result<T, Error>>,
    ) -> Result<T, Error> {
        let remaining: Duration = self.remaining()?;
        let result: Result<T, Error> = tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(cancelled()),
            result = tokio::time::timeout(remaining, future) => {
                result.map_err(|_| Error::new(ErrorKind::Timeout, "operation timed out; completion may be unknown"))?
            }
        };
        self.remaining()?;
        result
    }
}

pub struct SignalCancellation {
    token: CancellationToken,
    listener: JoinHandle<()>,
}

impl SignalCancellation {
    pub fn install() -> Result<Self, Error> {
        let mut signal: tokio::signal::unix::Signal =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .map_err(|_| Error::new(ErrorKind::Io, "cannot install interrupt handler"))?;
        let token: CancellationToken = CancellationToken::new();
        let listener_token: CancellationToken = token.clone();
        let listener: JoinHandle<()> = tokio::spawn(async move {
            signal.recv().await;
            listener_token.cancel();
        });
        Ok(Self { token, listener })
    }

    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}

impl Drop for SignalCancellation {
    fn drop(&mut self) {
        self.listener.abort();
    }
}

fn cancelled() -> Error {
    Error::new(
        ErrorKind::Cancelled,
        "operation cancelled; completion may be unknown",
    )
}

#[cfg(test)]
mod tests {
    use super::Operation;
    use crate::deadline::Deadline;
    use crate::error::ErrorKind;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn synchronous_work_cannot_report_success_after_the_deadline() {
        let operation: Operation = Operation::new(
            Deadline::new(Duration::from_millis(5)),
            CancellationToken::new(),
        );
        let error: crate::error::Error = operation
            .run(async {
                std::thread::sleep(Duration::from_millis(15));
                Ok(())
            })
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Timeout);
    }
}
