/**
 * Decks the native engine mixes. The renderer hands them out: players take them from 0 upwards and
 * the browser preview always has the last one, so raising this adds decks without moving any.
 * Four covers Prepare's deck, Mix's two and the preview.
 */
export const ENGINE_DECKS = 4
