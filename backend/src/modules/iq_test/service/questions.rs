use super::super::actions as OperationAction;
use super::*;

impl IqTest {
    pub async fn save_question(
        self: &Arc<Self>,
        input: IqQuestionInput,
        id: Option<String>,
    ) -> Result<IqQuestion> {
        self.blocking(move |this| {
            let action = if id.is_some() {
                OperationAction::IqQuestionUpdated
            } else {
                OperationAction::IqQuestionCreated
            };
            let mut state = this.state.lock().map_err(|_| IqError::Worker)?;
            let result = (|| {
                Self::ready(&state)?;
                if let Some(id) = &id {
                    validation::identifier(id, "id")?;
                }
                this.store
                    .save_question(id.as_deref(), validation::question(input)?)
            })();
            this.audit(action, "IQ 题库", &result);
            if result.is_ok() {
                this.changed(&mut state);
            }
            result
        })
        .await
    }
    pub async fn delete_question(self: &Arc<Self>, id: String) -> Result<()> {
        self.question_mutation(id, None).await
    }
    pub async fn set_question_enabled(self: &Arc<Self>, id: String, enabled: bool) -> Result<()> {
        self.question_mutation(id, Some(enabled)).await
    }
    async fn question_mutation(self: &Arc<Self>, id: String, enabled: Option<bool>) -> Result<()> {
        self.blocking(move |this| {
            let mut state = this.state.lock().map_err(|_| IqError::Worker)?;
            let action = if enabled.is_some() {
                OperationAction::IqQuestionUpdated
            } else {
                OperationAction::IqQuestionDeleted
            };
            let result = (|| {
                Self::ready(&state)?;
                validation::identifier(&id, "id")?;
                if let Some(enabled) = enabled {
                    this.store.set_question_enabled(&id, enabled)?;
                } else {
                    this.store.delete_question(&id)?;
                }
                Ok(())
            })();
            this.audit(action, &id, &result);
            if result.is_ok() {
                this.changed(&mut state);
            }
            result
        })
        .await
    }
}
