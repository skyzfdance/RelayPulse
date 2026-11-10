use super::super::actions as OperationAction;
use super::*;

impl IqTest {
    pub async fn snapshot(self: &Arc<Self>) -> Result<IqSnapshot> {
        self.blocking(|this| {
            let state = this.state.lock().map_err(|_| IqError::Worker)?;
            Self::ready(&state)?;
            let questions = this.store.questions()?;
            let runs = this
                .store
                .runs()?
                .into_iter()
                .filter(|r| r.status != RunStatus::Running)
                .collect();
            let candidates = this.candidates_with_health(this.source.candidates()?)?;
            let mut active_run = state.active.clone();
            if let (Some(run), Some(control)) = (&mut active_run, &state.control) {
                run.summarize(elapsed(control.started));
            }
            Ok(IqSnapshot {
                revision: state.revision,
                question_summary: IqQuestionSummary {
                    total: questions.len(),
                    enabled: questions.iter().filter(|q| q.enabled).count(),
                },
                questions,
                candidates,
                participants: state.participants.iter().map(|p| p.dto.clone()).collect(),
                active_run,
                runs,
            })
        })
        .await
    }
    pub async fn run_details(self: &Arc<Self>, id: String) -> Result<IqRun> {
        self.blocking(move |this| {
            validation::identifier(&id, "id")?;
            let state = this.state.lock().map_err(|_| IqError::Worker)?;
            Self::ready(&state)?;
            let mut run = this.store.run(&id)?;
            if let Some(control) = state.control.as_ref().filter(|c| c.id == id) {
                run.summarize(elapsed(control.started));
            }
            Ok(run)
        })
        .await
    }
    pub async fn history_options(self: &Arc<Self>) -> Result<Vec<IqHistoryFilterOption>> {
        self.blocking(|this| {
            let state = this.state.lock().map_err(|_| IqError::Worker)?;
            Self::ready(&state)?;
            let mut sites: BTreeMap<String, (String, BTreeMap<String, String>)> = BTreeMap::new();
            // runs 已按最近在前，同一历史身份采用最近快照名称。
            for run in this
                .store
                .runs()?
                .into_iter()
                .filter(|r| r.status != RunStatus::Running)
            {
                for participant in run.participants {
                    let p = participant.candidate;
                    let site = sites
                        .entry(p.site_id)
                        .or_insert_with(|| (p.site_name, BTreeMap::new()));
                    site.1.entry(p.model_key).or_insert(p.model_name);
                }
            }
            let mut options: Vec<_> = sites
                .into_iter()
                .map(|(value, (label, models))| {
                    let mut children: Vec<_> = models
                        .into_iter()
                        .map(|(value, label)| IqHistoryFilterOption {
                            value,
                            label,
                            children: None,
                        })
                        .collect();
                    sort_options(&mut children);
                    IqHistoryFilterOption {
                        value,
                        label,
                        children: Some(children),
                    }
                })
                .collect();
            sort_options(&mut options);
            Ok(options)
        })
        .await
    }
    pub async fn delete_runs(self: &Arc<Self>, ids: Vec<String>) -> Result<()> {
        self.blocking(move |this| {
            let mut state = this.state.lock().map_err(|_| IqError::Worker)?;
            Self::ready(&state)?;
            let mut ids = ids;
            ids.sort();
            ids.dedup();
            for id in &ids {
                validation::identifier(id, "ids")?;
                if state.active.as_ref().is_some_and(|r| r.id == *id) {
                    return Err(IqError::AlreadyRunning);
                }
                if this.store.contains_run(id)? && this.store.run(id)?.status == RunStatus::Running
                {
                    return Err(IqError::AlreadyRunning);
                }
            }
            for id in &ids {
                this.evidence.reconcile(id, this.store.contains_run(id)?)?;
            }
            for id in &ids {
                if let Err(error) = this.evidence.isolate(id) {
                    if !this.restore_all(&ids) {
                        return Err(IqError::RecoveryRequired);
                    }
                    return Err(error);
                }
            }
            if let Err(error) = this.store.delete_runs(&ids) {
                if !this.restore_all(&ids) {
                    return Err(IqError::RecoveryRequired);
                }
                return Err(error);
            }
            this.changed(&mut state);
            let mut pending = false;
            for id in &ids {
                let result = this.evidence.cleanup(id);
                this.audit(OperationAction::IqRunDeleted, id, &result);
                pending |= result.is_err();
            }
            if pending {
                Err(IqError::CleanupPending)
            } else {
                Ok(())
            }
        })
        .await
    }
    pub async fn open_raw(
        self: &Arc<Self>,
        entry_id: String,
        attempt: Option<u32>,
        open: impl FnOnce(&Path) -> Result<()> + Send + 'static,
    ) -> Result<()> {
        self.blocking(move |this| {
            validation::identifier(&entry_id, "entryId")?;
            let state = this.state.lock().map_err(|_| IqError::Worker)?;
            Self::ready(&state)?;
            let entry = this
                .store
                .runs()?
                .into_iter()
                .flat_map(|r| r.entries)
                .find(|e| e.id == entry_id)
                .ok_or(IqError::NotFound)?;
            let path = match attempt {
                None => this.evidence.raw_path(&entry)?,
                Some(attempt) => this.evidence.raw_attempt_path(&entry, Some(attempt))?,
            };
            open(path.parent().ok_or(IqError::Evidence)?)
        })
        .await
    }
    fn restore_all(&self, ids: &[String]) -> bool {
        let mut restored = true;
        for id in ids {
            restored &= self.evidence.restore(id).is_ok();
        }
        restored
    }
}

fn sort_options(options: &mut [IqHistoryFilterOption]) {
    options.sort_by(|a, b| {
        a.label
            .to_lowercase()
            .cmp(&b.label.to_lowercase())
            .then_with(|| a.value.cmp(&b.value))
    });
}
