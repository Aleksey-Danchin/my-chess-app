export const DOWNLOAD_CANCELLED = "cancelled";

export function isDownloadCancelled(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return message === DOWNLOAD_CANCELLED || message.includes(DOWNLOAD_CANCELLED);
}
