export type LevelCurve = (inputLevel: number) => number

export function softClip(drive: number, scale: number): LevelCurve {
  const fullScale = Math.tanh(drive)
  return (inputLevel) => Math.tanh(drive * inputLevel) / (fullScale * scale)
}

const unchanged: LevelCurve = (inputLevel) => inputLevel

// const BAND_CURVES_LOW_TO_HIGH: readonly LevelCurve[] = [unchanged, softClip(1.3), unchanged]
const BAND_CURVES_LOW_TO_HIGH: readonly LevelCurve[] = [softClip(1, 1.4), softClip(1.3, 1.4), softClip(0.01, 1.4)]

export function shapeBandLevel(band: number, level: number): number {
  return BAND_CURVES_LOW_TO_HIGH[band](level)
}
