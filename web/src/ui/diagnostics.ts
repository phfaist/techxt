import type { BusyState, DiagnosticsInit, DiagnosticsPanel } from './api';
import type { ConversionResult } from '../worker/protocol';

/** Stub — replaced by the real implementation. */
export function initDiagnostics(_init: DiagnosticsInit): DiagnosticsPanel {
  return {
    setResult(_result: ConversionResult) {},
    setBusy(_state: BusyState) {},
    setNote(_note: string | null) {},
    setOpen(_open: boolean) {},
  };
}
