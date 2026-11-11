import type { Component } from 'svelte'
import {
  AudioWaveform,
  Blend,
  Database,
  FileAudio,
  Info,
  Keyboard,
  Monitor,
  Palette,
  SlidersHorizontal,
  Wrench
} from '@lucide/svelte'
import AboutSettings from './AboutSettings.svelte'
import AnalysisSettings from './AnalysisSettings.svelte'
import ConvertSettings from './ConvertSettings.svelte'
import DatabaseSettings from './DatabaseSettings.svelte'
import DeveloperSettings from './DeveloperSettings.svelte'
import DisplaySettings from './DisplaySettings.svelte'
import GeneralSettings from './GeneralSettings.svelte'
import KeyboardSettings from './KeyboardSettings.svelte'
import MixSettings from './MixSettings.svelte'
import ThemeSettings from './ThemeSettings.svelte'

export type SettingsSection =
  'general' | 'display' | 'theme' | 'mix' | 'analysis' | 'convert' | 'database' | 'keyboard' | 'developer' | 'about'

export type SectionInfo = {
  id: SettingsSection
  label: string
  icon: typeof Keyboard
  page: Component<{ title: string }>
}

export const SECTIONS: SectionInfo[] = [
  { id: 'general', label: 'General', icon: SlidersHorizontal, page: GeneralSettings },
  { id: 'display', label: 'Display', icon: Monitor, page: DisplaySettings },
  { id: 'theme', label: 'Theme & colors', icon: Palette, page: ThemeSettings },
  { id: 'mix', label: 'Mix', icon: Blend, page: MixSettings },
  { id: 'analysis', label: 'Import & analysis', icon: AudioWaveform, page: AnalysisSettings },
  { id: 'convert', label: 'Convert formats', icon: FileAudio, page: ConvertSettings },
  { id: 'database', label: 'Database & files', icon: Database, page: DatabaseSettings },
  { id: 'keyboard', label: 'Keyboard shortcuts', icon: Keyboard, page: KeyboardSettings },
  { id: 'developer', label: 'Developer options', icon: Wrench, page: DeveloperSettings }
]

export const ABOUT: SectionInfo = { id: 'about', label: 'About', icon: Info, page: AboutSettings }
