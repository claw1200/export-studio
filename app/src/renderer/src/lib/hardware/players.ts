/** What one player can load from a USB stick, taken from its instruction manual. */

export type AudioFormat = 'mp3' | 'aac' | 'wav' | 'aiff' | 'flac' | 'alac'

export interface FormatLimit {
  /** Hertz. */
  rates: readonly number[]
  /** Quantization bits. Empty means this format is not judged on bit depth. */
  bits: readonly number[]
  /** Inclusive kilobits per second. */
  bitrate?: readonly [number, number]
}

export interface Player {
  id: string
  name: string
  year: number
  formats: Partial<Record<AudioFormat, FormatLimit>>
}

const CD = [44_100, 48_000] as const
const HI = [44_100, 48_000, 88_200, 96_000] as const
const PCM = [16, 24] as const

const mp3: FormatLimit = { rates: CD, bits: [], bitrate: [32, 320] }
const aac: FormatLimit = { rates: CD, bits: [16], bitrate: [16, 320] }
/** The MEP-7000 manual allows AAC from 8 kbps. */
const aacFrom8: FormatLimit = { rates: CD, bits: [16], bitrate: [8, 320] }
const pcm48: FormatLimit = { rates: CD, bits: PCM }
const pcm96: FormatLimit = { rates: HI, bits: PCM }
/** 16-bit PCM at 44.1 kHz, which is all the MEP-7000 plays for WAV and AIFF. */
const pcm4416: FormatLimit = { rates: [44_100], bits: [16] }

/** MP3, AAC, and 16/24-bit WAV, AIFF, FLAC, and ALAC at 44.1 or 48 kHz. */
const wide48: Player['formats'] = { mp3, aac, wav: pcm48, aiff: pcm48, flac: pcm48, alac: pcm48 }
/** The same formats, with PCM and lossless up to 96 kHz, including 88.2. */
const wide96: Player['formats'] = { mp3, aac, wav: pcm96, aiff: pcm96, flac: pcm96, alac: pcm96 }
/** MP3, AAC, and 16/24-bit WAV, AIFF, and FLAC at 44.1 or 48 kHz. No ALAC. */
const flac48: Player['formats'] = { mp3, aac, wav: pcm48, aiff: pcm48, flac: pcm48 }
/** MP3, AAC, and 16/24-bit WAV and AIFF at 44.1 or 48 kHz. No FLAC or ALAC. */
const pcmClub: Player['formats'] = { mp3, aac, wav: pcm48, aiff: pcm48 }

/**
 * Newest first. `year` is the release year, which is what "newer" means for the hardware light.
 * Limits are the USB playback table in each instruction manual. Where a manual also lists
 * 32 kHz MP3 or AAC, the check still uses 44.1 and 48 kHz, the rates these players share.
 */
export const PLAYERS: readonly Player[] = [
  { id: 'cdj-1500x', name: 'CDJ-1500X', year: 2026, formats: wide48 },
  { id: 'cdj-3000x', name: 'CDJ-3000X', year: 2024, formats: wide96 },
  { id: 'xdj-az', name: 'XDJ-AZ', year: 2024, formats: wide96 },
  { id: 'omnis-duo', name: 'OMNIS-DUO', year: 2024, formats: wide48 },
  { id: 'opus-quad', name: 'OPUS-QUAD', year: 2023, formats: wide96 },
  { id: 'xdj-rx3', name: 'XDJ-RX3', year: 2021, formats: flac48 },
  { id: 'cdj-3000', name: 'CDJ-3000', year: 2020, formats: wide96 },
  { id: 'xdj-xz', name: 'XDJ-XZ', year: 2019, formats: flac48 },
  { id: 'xdj-rr', name: 'XDJ-RR', year: 2018, formats: pcmClub },
  { id: 'cdj-2000nxs2', name: 'CDJ-2000NXS2', year: 2017, formats: wide96 },
  { id: 'xdj-rx2', name: 'XDJ-RX2', year: 2017, formats: pcmClub },
  { id: 'cdj-tour1', name: 'CDJ-TOUR1', year: 2016, formats: wide96 },
  { id: 'xdj-1000mk2', name: 'XDJ-1000MK2', year: 2016, formats: wide48 },
  { id: 'xdj-rx', name: 'XDJ-RX', year: 2015, formats: pcmClub },
  { id: 'xdj-700', name: 'XDJ-700', year: 2015, formats: pcmClub },
  { id: 'cdj-2000nxs', name: 'CDJ-2000NXS', year: 2014, formats: pcmClub },
  { id: 'xdj-1000', name: 'XDJ-1000', year: 2014, formats: pcmClub },
  { id: 'xdj-r1', name: 'XDJ-R1', year: 2013, formats: pcmClub },
  { id: 'cdj-900nxs', name: 'CDJ-900NXS', year: 2013, formats: pcmClub },
  { id: 'xdj-aero', name: 'XDJ-AERO', year: 2012, formats: pcmClub },
  { id: 'cdj-850', name: 'CDJ-850', year: 2011, formats: pcmClub },
  { id: 'cdj-900', name: 'CDJ-900', year: 2010, formats: pcmClub },
  { id: 'cdj-350', name: 'CDJ-350', year: 2010, formats: pcmClub },
  { id: 'cdj-2000', name: 'CDJ-2000', year: 2009, formats: pcmClub },
  { id: 'cdj-400', name: 'CDJ-400', year: 2008, formats: { mp3 } },
  {
    id: 'mep-7000',
    name: 'MEP-7000',
    year: 2008,
    formats: { mp3, aac: aacFrom8, wav: pcm4416, aiff: pcm4416 }
  }
]

const byId = new Map(PLAYERS.map((player) => [player.id, player]))

export function playerById(id: string): Player | undefined {
  return byId.get(id)
}

export const DEFAULT_GREEN = 'xdj-1000mk2'
