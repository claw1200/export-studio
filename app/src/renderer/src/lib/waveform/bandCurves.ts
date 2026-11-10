type CurvePoint = readonly [inputLevel: number, outputLevel: number]

/** Points from input level 0 to 1, in order; levels between them are joined by straight lines. */
export type LevelCurve = readonly CurvePoint[]

const UNCHANGED: LevelCurve = [
  [0, 0],
  [1, 1]
]

const QUIETER_HIGHS: LevelCurve = [
  [0, 0],
  [1, 0.6]
]

const LIFTED_QUIET_MIDS: LevelCurve = [
  [0, 0],
  [0.2, 0.32],
  [0.5, 0.65],
  [0.8, 0.88],
  [1, 1]
]

const BAND_CURVES_LOW_TO_HIGH: readonly LevelCurve[] = [UNCHANGED, LIFTED_QUIET_MIDS, UNCHANGED]

export function followCurve(curve: LevelCurve, inputLevel: number): number {
  let upper = 1
  while (upper < curve.length - 1 && inputLevel > curve[upper][0]) upper++
  const [fromInput, fromOutput] = curve[upper - 1]
  const [toInput, toOutput] = curve[upper]
  return fromOutput + ((inputLevel - fromInput) / (toInput - fromInput)) * (toOutput - fromOutput)
}

export function shapeBandLevel(band: number, level: number): number {
  return followCurve(BAND_CURVES_LOW_TO_HIGH[band], level)
}
