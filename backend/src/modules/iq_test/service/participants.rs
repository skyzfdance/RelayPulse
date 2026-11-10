use super::*;

impl IqTest {
    pub async fn create_participant(
        self: &Arc<Self>,
        input: IqParticipantInput,
    ) -> Result<IqParticipant> {
        self.blocking(move |this| {
            let mut state = this.state.lock().map_err(|_| IqError::Worker)?;
            Self::idle(&state)?;
            let execution = this.participant(input, uuid::Uuid::new_v4().to_string(), None)?;
            let dto = execution.dto.clone();
            state.participants.push(execution);
            this.changed(&mut state);
            Ok(dto)
        })
        .await
    }
    fn participant(
        &self,
        input: IqParticipantInput,
        id: String,
        previous: Option<&Sanitizer>,
    ) -> Result<ParticipantExecution> {
        super::super::validation::parameters(&input.parameters)?;
        if input.preset.is_empty() || input.preset.chars().count() > 256 {
            return Err(invalid(
                "input.preset",
                "IQ 参数展示标签须为 1–256 个字符。",
            ));
        }
        let connection = self.source.snapshot(&input.model_key)?;
        let sanitize = Sanitizer::new(&connection, previous);
        let mut endpoint = crate::shared::model_client::endpoint(
            &connection.target.base_url,
            &connection.endpoint,
            false,
        )
        .map_err(|_| invalid("input.modelKey", "源连接端点无效。"))?;
        endpoint.set_query(None);
        endpoint.set_fragment(None);
        let mut source = self
            .candidates_with_health(vec![connection.source.clone()])?
            .pop()
            .ok_or(IqError::Worker)?;
        source.model_name = sanitize.text(&source.model_name);
        source.site_name = sanitize.text(&source.site_name);
        let dto = IqParticipant {
            candidate: source,
            id,
            snapshot_id: uuid::Uuid::new_v4().to_string(),
            endpoint: sanitize.text(endpoint.as_str()),
            created_at_ms: now_ms(),
            preset: sanitize.text(&input.preset),
            parameters: sanitize
                .value(&Value::Object(input.parameters.clone()))
                .as_object()
                .cloned()
                .ok_or(IqError::Worker)?,
            precheck: IqPrecheck {
                status: PrecheckStatus::Checking,
                message: String::new(),
                checked_at_ms: None,
            },
        };
        Ok(ParticipantExecution {
            dto,
            connection,
            parameters: input.parameters,
            sanitize,
        })
    }
    pub async fn refresh_participant(self: &Arc<Self>, id: String) -> Result<IqParticipant> {
        self.blocking(move |this| {
            validation::identifier(&id, "id")?;
            let mut state = this.state.lock().map_err(|_| IqError::Worker)?;
            Self::idle(&state)?;
            let index = state
                .participants
                .iter()
                .position(|p| p.dto.id == id)
                .ok_or(IqError::NotFound)?;
            let old = &state.participants[index];
            if old.dto.precheck.status != PrecheckStatus::Unavailable {
                return Err(invalid("id", "仅预检失败后可以重新建立执行快照。"));
            }
            let next = this.participant(
                IqParticipantInput {
                    model_key: old.dto.candidate.model_key.clone(),
                    preset: old.dto.preset.clone(),
                    parameters: old.parameters.clone(),
                },
                id,
                Some(&old.sanitize),
            )?;
            let dto = next.dto.clone();
            state.participants[index] = next;
            this.changed(&mut state);
            Ok(dto)
        })
        .await
    }
    pub async fn remove_participant(self: &Arc<Self>, id: String) -> Result<()> {
        self.blocking(move |this| {
            validation::identifier(&id, "id")?;
            let mut state = this.state.lock().map_err(|_| IqError::Worker)?;
            Self::idle(&state)?;
            if let Some(index) = state.participants.iter().position(|p| p.dto.id == id) {
                let old = state.participants.remove(index);
                if let Some(job) = state.prechecks.get(&old.dto.snapshot_id) {
                    job.cancellation.cancel();
                }
                this.changed(&mut state);
            }
            Ok(())
        })
        .await
    }
    pub async fn precheck(self: &Arc<Self>, snapshot_id: String) -> Result<IqPrecheckResult> {
        let runtime = Handle::current();
        let target = snapshot_id.clone();
        let mut result = self
            .blocking(move |this| {
                validation::identifier(&target, "snapshotId")?;
                let mut state = this.state.lock().map_err(|_| IqError::Worker)?;
                Self::ready(&state)?;
                let participant = state
                    .participants
                    .iter()
                    .find(|p| p.dto.snapshot_id == target)
                    .cloned()
                    .ok_or(IqError::NotFound)?;
                if participant.dto.precheck.status != PrecheckStatus::Checking {
                    return Ok(watch::channel(Some(participant.dto.precheck)).1);
                }
                if let Some(job) = state.prechecks.get(&target) {
                    return Ok(job.result.clone());
                }
                let (sender, receiver) = watch::channel(None);
                let cancellation = Cancellation::default();
                state.prechecks.insert(
                    target.clone(),
                    PrecheckJob {
                        cancellation: cancellation.clone(),
                        result: receiver.clone(),
                    },
                );
                let service = this.clone();
                this.spawn(
                    async move {
                        let result = std::panic::AssertUnwindSafe(
                            service.check_snapshot(&participant, &cancellation),
                        )
                        .catch_unwind()
                        .await;
                        let precheck = result.unwrap_or_else(|_| {
                            precheck::unavailable("预检任务未能完成，请重新探测。")
                        });
                        if let Ok(mut state) = service.state.lock() {
                            state.prechecks.remove(&target);
                            if let Some(current) = state
                                .participants
                                .iter_mut()
                                .find(|p| p.dto.snapshot_id == target)
                            {
                                current.dto.precheck = precheck.clone();
                                service.changed(&mut state);
                            }
                        }
                        sender.send_replace(Some(precheck));
                    },
                    &runtime,
                )?;
                Ok(receiver)
            })
            .await?;
        loop {
            if let Some(precheck) = result.borrow_and_update().clone() {
                return Ok(IqPrecheckResult {
                    snapshot_id,
                    precheck,
                });
            }
            result.changed().await.map_err(|_| IqError::Worker)?;
        }
    }
    pub(super) fn candidates_with_health(
        &self,
        sources: Vec<IqModelSource>,
    ) -> Result<Vec<IqCandidate>> {
        let ids: Vec<_> = sources
            .iter()
            .filter_map(|s| s.model_key.split_once('/').map(|(_, id)| id.to_owned()))
            .collect();
        let health = self.health.read(&ids)?;
        Ok(sources
            .into_iter()
            .map(|source| {
                let status = source
                    .model_key
                    .split_once('/')
                    .and_then(|(_, id)| health.get(id))
                    .cloned()
                    .unwrap_or_else(|| "muted".into());
                candidate(source, status)
            })
            .collect())
    }
}

fn candidate(source: IqModelSource, health: String) -> IqCandidate {
    IqCandidate {
        model_key: source.model_key,
        model_name: source.model_name,
        site_id: source.site_id,
        site_name: source.site_name,
        protocol: source.protocol,
        health,
    }
}
