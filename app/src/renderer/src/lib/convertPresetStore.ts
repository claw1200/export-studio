import { CONVERT_PRESETS_KEY, parsePresets } from './convertPresets'
import { getSetting, setSetting } from './settings'
import type { ConvertPreset } from './types'

export function loadPresets(): ConvertPreset[] {
  return parsePresets(getSetting(CONVERT_PRESETS_KEY))
}

export function savePresets(presets: ConvertPreset[]): void {
  setSetting(CONVERT_PRESETS_KEY, JSON.stringify(presets))
}
