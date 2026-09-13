import type { I18nValue, MessageKey } from "../i18n/i18n";

const INTERNAL_OPERATION_FAILURE_PREFIX = "control-center-internal-operation-failed:";
const CODED_ERROR_MESSAGES = [
  ["control-center-installation-required:", "execution.servicePackageNotInstalledDescription"],
  ["control-center-installation-incomplete:", "execution.servicePackageIncompleteDescription"],
  ["control-center-installation-untrusted:", "execution.servicePackageUntrustedDescription"],
  ["control-center-default-profile-missing:", "execution.defaultProfileMissing"],
  ["control-center-default-profile-invalid:", "execution.defaultProfileInvalid"],
] as const satisfies ReadonlyArray<readonly [string, MessageKey]>;

export function operationErrorMessage(
  caught: unknown,
  t: I18nValue["t"],
  internalMessage: MessageKey = "execution.operationFailed",
): string {
  const detail = caught instanceof Error ? caught.message : String(caught);
  const codedMessage = CODED_ERROR_MESSAGES.find(([prefix]) => detail.startsWith(prefix));
  if (codedMessage) return t(codedMessage[1]);
  return detail.startsWith(INTERNAL_OPERATION_FAILURE_PREFIX) ? t(internalMessage) : detail;
}
