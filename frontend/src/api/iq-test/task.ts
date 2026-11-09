import type {
  IqParticipant,
  IqParticipantInput,
  IqPrecheck,
  IqRun,
  IqStartInput,
} from '@/types/domain/iq-test';

import { callBackend } from '../call-backend';

export function createIqParticipant(input: IqParticipantInput): Promise<IqParticipant> {
  return callBackend<IqParticipant>(
    'create_iq_participant_snapshot',
    { input },
    { successFeedback: 'none', failFeedback: 'message' },
  );
}

export function replaceIqParticipant(id: string): Promise<IqParticipant> {
  return callBackend<IqParticipant>(
    'refresh_iq_participant_snapshot',
    { id },
    { successFeedback: 'none', failFeedback: 'message' },
  );
}

/** 预检成功、失败均静默，失败原因只在对应快照行内展示。 */
export function runIqPrecheck(
  snapshotId: string,
): Promise<{ snapshotId: string; precheck: IqPrecheck }> {
  return callBackend<{ snapshotId: string; precheck: IqPrecheck }>(
    'run_iq_precheck',
    { snapshotId },
    { successFeedback: 'none', failFeedback: 'none' },
  );
}

export function removeIqParticipant(id: string): Promise<void> {
  return callBackend<void>(
    'remove_iq_participant',
    { id },
    { successFeedback: 'none', failFeedback: 'message' },
  );
}

export function startIqRun(input: IqStartInput): Promise<IqRun> {
  return callBackend<IqRun>(
    'start_iq_run',
    { input },
    { successFeedback: 'none', failFeedback: 'message' },
  );
}

export function abortIqRun(): Promise<void> {
  return callBackend<void>('abort_iq_run', undefined, {
    successFeedback: 'none',
    failFeedback: 'message',
  });
}
