use super::super::actions as OperationAction;
use super::*;

impl IqTest {
    pub async fn start_run(self: &Arc<Self>, input: IqStartInput) -> Result<IqRun> {
        let runtime = Handle::current();
        let retry_max = input.retry_max;
        self.blocking(move |this| {
            let mut state = this.state.lock().map_err(|_| IqError::Worker)?;
            let result = (|| {
                Self::idle(&state)?;
                validation::start(&input)?;
                if state.participants.is_empty()
                    || state
                        .participants
                        .iter()
                        .any(|p| p.dto.precheck.status != PrecheckStatus::Available)
                {
                    return Err(invalid(
                        "participants",
                        "每位参与者的当前执行快照均须预检可用。",
                    ));
                }
                this.store.create_run(
                    input,
                    state.participants.iter().map(|p| p.dto.clone()).collect(),
                )
            })();
            this.audit(OperationAction::IqRunStarted, "IQ 测试", &result);
            let run = result?;
            let control = Arc::new(RunControl {
                id: run.id.clone(),
                retry_max,
                cancellation: Cancellation::default(),
                done: watch::channel(false).0,
                started: Instant::now(),
                user_abort: std::sync::atomic::AtomicBool::new(false),
            });
            let participants = state.participants.clone();
            state.active = Some(run.clone());
            state.control = Some(control.clone());
            this.changed(&mut state);
            let service = this.clone();
            this.spawn(
                async move {
                    let done = Done(control.done.clone());
                    let outcome = std::panic::AssertUnwindSafe(
                        service.clone().run(participants, control.clone()),
                    )
                    .catch_unwind()
                    .await;
                    if outcome.is_err() {
                        control.cancellation.cancel();
                    }
                    let _ = service.finish_run(control, outcome.is_err()).await;
                    drop(done);
                },
                &runtime,
            )?;
            Ok(run)
        })
        .await
    }
    pub async fn abort_run(self: &Arc<Self>) -> Result<()> {
        let control = {
            let state = self.state.lock().map_err(|_| IqError::Worker)?;
            Self::ready(&state)?;
            state.control.clone()
        };
        let Some(control) = control else {
            return Ok(());
        };
        control
            .user_abort
            .store(true, std::sync::atomic::Ordering::SeqCst);
        control.cancellation.cancel();
        let mut done = control.done.subscribe();
        while !*done.borrow_and_update() {
            done.changed().await.map_err(|_| IqError::Worker)?;
        }
        self.blocking(move |this| {
            let state = this.state.lock().map_err(|_| IqError::Worker)?;
            if state.unavailable {
                Err(IqError::Storage)
            } else {
                Ok(())
            }
        })
        .await
    }
}
