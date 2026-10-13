import { DEFAULT_GREEN, PLAYERS, playerById, type AudioFormat, type Player } from './players'
import type { AudioProbe, Track } from '../types'

export type Light = 'green' | 'yellow' | 'red'

export interface AudioFacts {
  kind: string | null
  sampleRate: number
  bitDepth: number
  channels: number | null
  bitrate: number
  encoding: AudioProbe['encoding'] | null
}

export interface Verdict {
  color: Light
  /** Why the light is green, or why the file itself cannot be loaded. Empty when the light is yellow. */
  summary: string
  /** Distinct reasons some player cannot load the file. */
  problems: string[]
  /** Every player that cannot load the file, newest first. */
  incompatible: string[]
}

const NAMES: Record<AudioFormat, string> = {
  mp3: 'MP3',
  aac: 'AAC',
  wav: 'WAV',
  aiff: 'AIFF',
  flac: 'FLAC',
  alac: 'ALAC'
}

function khz(rate: number): string {
  if (rate % 1000 === 0) return `${rate / 1000} kHz`
  if (rate % 100 === 0) return `${(rate / 1000).toFixed(1)} kHz`
  return `${(rate / 1000).toFixed(2)} kHz`
}

function formatOf(kind: string | null, encoding: AudioFacts['encoding']): AudioFormat | null {
  const name = (kind ?? '').toUpperCase()
  if (name === 'MP3' || encoding === 'mp3') return 'mp3'
  if (name === 'FLAC' || encoding === 'flac') return 'flac'
  if (name === 'ALAC' || encoding === 'alac') return 'alac'
  if (name === 'WAV') return 'wav'
  if (name === 'AIFF' || name === 'AIF') return 'aiff'
  if (name === 'AAC' || name === 'M4A' || encoding === 'aac') return 'aac'
  return null
}

/** Why this player cannot load the file, or null when it can. */
function failure(player: Player, facts: AudioFacts): { rank: number; text: string } | null {
  if (facts.encoding === 'float') {
    return { rank: 1, text: facts.bitDepth ? `${facts.bitDepth}-bit float` : '32-bit float' }
  }
  if (facts.encoding === 'other') return { rank: 2, text: 'Not integer PCM' }
  if (facts.channels != null && facts.channels > 0 && facts.channels !== 1 && facts.channels !== 2) {
    return { rank: 3, text: `${facts.channels} channels` }
  }
  const format = formatOf(facts.kind, facts.encoding)
  if (!format) return { rank: 0, text: 'Unknown file type' }
  const limit = player.formats[format]
  const label = NAMES[format]
  if (!limit) return { rank: 4, text: `${label} file` }
  if (facts.sampleRate > 0 && !limit.rates.includes(facts.sampleRate)) {
    return { rank: 5, text: `${label} file is ${khz(facts.sampleRate)}` }
  }
  if (facts.bitDepth > 0 && limit.bits.length > 0 && !limit.bits.includes(facts.bitDepth)) {
    return { rank: 6, text: `${facts.bitDepth}-bit ${label} file` }
  }
  if (facts.bitrate > 0 && limit.bitrate && (facts.bitrate < limit.bitrate[0] || facts.bitrate > limit.bitrate[1])) {
    return { rank: 7, text: `${facts.bitrate} kbps ${label}` }
  }
  return null
}

function target(id: string, fallback: string): Player {
  return playerById(id) ?? playerById(fallback)!
}

function collect(players: readonly Player[], facts: AudioFacts): { problems: string[]; incompatible: string[] } {
  const reasons = new Map<string, number>()
  const incompatible: string[] = []
  const ordered = [...players].sort((a, b) => b.year - a.year || a.name.localeCompare(b.name))
  for (const player of ordered) {
    const reason = failure(player, facts)
    if (!reason) continue
    incompatible.push(player.name)
    if (!reasons.has(reason.text)) reasons.set(reason.text, reason.rank)
  }
  const problems = [...reasons.entries()].sort((a, b) => a[1] - b[1] || a[0].localeCompare(b[0])).map(([text]) => text)
  return { problems, incompatible }
}

export function factsFromTrack(track: Pick<Track, 'fileType' | 'sampleRate' | 'bitDepth' | 'bitrate'>): AudioFacts {
  return {
    kind: track.fileType || null,
    sampleRate: track.sampleRate,
    bitDepth: track.bitDepth,
    channels: null,
    bitrate: track.bitrate,
    encoding: null
  }
}

export function factsFromProbe(probe: AudioProbe, track: Pick<Track, 'bitrate'>): AudioFacts {
  return {
    kind: probe.kind,
    sampleRate: probe.sampleRate,
    bitDepth: probe.bitDepth,
    channels: probe.channels,
    bitrate: probe.bitrate > 0 ? probe.bitrate : track.bitrate,
    encoding: probe.encoding
  }
}

export function assess(
  facts: AudioFacts,
  options: { missing?: boolean; unreadable?: boolean; streaming?: boolean; greenId: string }
): Verdict {
  const empty = { problems: [], incompatible: [] }
  if (options.missing) return { color: 'red', summary: 'File missing', ...empty }
  if (options.unreadable) return { color: 'red', summary: "File can't be read", ...empty }
  if (options.streaming) {
    return { color: 'yellow', summary: 'Streaming track', problems: ['No file to load'], incompatible: [] }
  }

  const green = target(options.greenId, DEFAULT_GREEN)
  const modern = PLAYERS.filter((player) => player.id === green.id || player.year > green.year)
  const plays = modern.every((player) => failure(player, facts) == null)
  const { problems, incompatible } = collect(PLAYERS, facts)
  return {
    color: plays ? 'green' : 'yellow',
    summary: plays ? `Plays on ${green.name} and newer` : '',
    problems,
    incompatible
  }
}
