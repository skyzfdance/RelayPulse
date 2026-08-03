export function isNewerRevision(currentRevision: number | null, incomingRevision: number) {
  return currentRevision === null || incomingRevision > currentRevision;
}
