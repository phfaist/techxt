import type { Panes, PanesInit } from './api';
import type { FontId } from '../fonts';

/** Stub — replaced by the real implementation. */
export function initPanes(init: PanesInit): Panes {
  const input = document.createElement('textarea');
  init.mount.appendChild(input);
  return {
    input,
    getDocument: () => input.value,
    setDocument(text: string) { input.value = text; },
    setOutput(_text: string) {},
    getOutput: () => '',
    selectSpan(_start: number, _end: number) {},
    columns: () => 72,
    async setFont(_font: FontId, _size: number) {},
    setSplit(_split: number) {},
    setFocus(_focus: 'input' | 'output' | null) {},
    setFontLoading(_loading: boolean) {},
    setStale(_stale: boolean) {},
  };
}
