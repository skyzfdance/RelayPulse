import type { IqQuestion, IqQuestionInput } from '@/types/domain/iq-test';

import { callBackend } from '../call-backend';

/** 保存题目，失败由题库编辑抽屉展示。 */
export function saveIqQuestion(input: IqQuestionInput, id?: string): Promise<IqQuestion> {
  return callBackend<IqQuestion>(
    'save_iq_question',
    { input, id },
    { successFeedback: 'none', failFeedback: 'none' },
  );
}

/** 删除题目，失败由题库列表展示。 */
export function deleteIqQuestion(id: string): Promise<void> {
  return callBackend<void>(
    'delete_iq_question',
    { id },
    { successFeedback: 'none', failFeedback: 'none' },
  );
}

/** 修改题目启停状态，失败由题库列表展示。 */
export function setIqQuestionEnabled(id: string, enabled: boolean): Promise<void> {
  return callBackend<void>(
    'set_iq_question_enabled',
    { id, enabled },
    { successFeedback: 'none', failFeedback: 'none' },
  );
}
