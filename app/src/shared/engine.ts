/**
 * Decks the native engine mixes. The renderer hands them out: players take them from 0 upwards and
 * the browser preview always has the last one, so raising this adds decks without moving any.
 * Four covers Edit's deck, Mix's two and the preview.
 */
export const ENGINE_DECKS = 4

/** Slowest and fastest a deck plays, as a multiple of the file's speed (the engine clamps to these). */
export const MIN_TEMPO = 0.5
export const MAX_TEMPO = 2

/** Furthest a deck's key can move either way, in semitones. */
export const MAX_KEY_SHIFT = 12
