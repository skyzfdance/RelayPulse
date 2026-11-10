#![allow(non_upper_case_globals)]
use relaypulse_operations::ActionDefinition;
pub const IqQuestionCreated: ActionDefinition =
    ActionDefinition::new("iq_question_created", "新建 IQ 题目", "create");
pub const IqQuestionUpdated: ActionDefinition =
    ActionDefinition::new("iq_question_updated", "修改 IQ 题目", "update");
pub const IqQuestionDeleted: ActionDefinition =
    ActionDefinition::new("iq_question_deleted", "删除 IQ 题目", "delete");
pub const IqRunStarted: ActionDefinition =
    ActionDefinition::new("iq_run_started", "开始 IQ 测试", "create");
pub const IqRunAborted: ActionDefinition =
    ActionDefinition::new("iq_run_aborted", "中止 IQ 测试", "status_change");
pub const IqRunDeleted: ActionDefinition =
    ActionDefinition::new("iq_run_deleted", "删除 IQ 测试历史", "delete");
