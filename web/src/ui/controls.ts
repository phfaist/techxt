import type { Controls, ControlsInit } from './api';
import type { AppOptions } from '../types';
import type { FontId } from '../fonts';

/** Stub — replaced by the real implementation. */
export function initControls(_init: ControlsInit): Controls {
  return {
    setOptions(_options: AppOptions) {},
    setFont(_font: FontId, _size: number) {},
    setMoreOpen(_open: boolean) {},
    setKeepFontsOffline(_enabled: boolean) {},
    close() {},
  };
}
