# Working in this repo

## Code philosophy

- **DRY (Don't Repeat Yourself).** Eliminate duplication by creating reusable, consistent components and shared logic. Maintain a single source of truth so changes are made once, reducing complexity, errors and maintenance overhead.

- **Self-Documenting Code (strict).** Write code that clearly communicates its intent through meaningful names, simple structures and expressive methods. Code should explain what it does and why without relying on excessive comments or external documentation.

- **Clarity, Consistency and Maintainability.** Prioritize readable, reusable and intuitive code over clever or overly complex solutions. Make it easy for others (and your future self) to understand, extend and maintain the code with confidence.

- **Leave the codebase better than you found it.** If working in a branch or PR, tidy up code surrounding your change. Don't just stick in the feature and force it to work in the current environment, consider the architecture and how your changes effect it. Assess if the design or maintainability of the codebase is improved or worsened by your change, and vice versa see if your feature would benefit from a refactor of the surrounding code. If you see a better way to do something, at least consider it.

## When you touch code

The codebase has too many comments and too many vague names, and we fix them as we go. These rules apply to the area you edit: the functions you change, plus their direct callers and callees. They also apply to every function in a diff you review. Only go through a whole file when the task is a refactor. If you spot problems outside the area, mention them; don't fix them.

[`deck/deck.svelte.ts`](app/src/renderer/src/lib/deck/deck.svelte.ts) is the reference: it has no comments, its types carry the units, its names carry the intent, and small helpers remove the repetition.

**Start from deletion.** For each comment, docstrings included, ask "could the code say this?", not "is this comment true or useful?". If it could, change the code and delete the comment. Keep a comment only for a *why* the code can't express: a non-obvious reason, an outside constraint, or domain jargon.

Ways to make a comment unnecessary:
- **Units:** use a named type (`Seconds`) or a parameter name (`linearGain`) instead of "in seconds" or "0 to 1".
- **A condition:** give it a name (`const playheadOutside = …`, `const predatesLastCommand = …`).
- **A magic number:** make it a named constant (`LOOP_EDGE_TOLERANCE`).
- **A special value or flag:** split the function (`setLow(null)` → `bypassIsolator()`; `play({ synced })` → `play({ onMasterBeat })`).
- **A hidden return value:** split it into an action and a query (`markPlaying()` + `canStartNow`).

Checks on the design:
- **Judge a name where it's called.** Named imports drop the module, so a name must make sense without its file.
- **Read every caller.** If callers undo part of what a function returns, narrow the function. If callers all compute its input the same way, let it take the raw values.
- **Name a repeated sequence once** (`sendOrReplayAfterLoad`, `reanchor`), even if it's only two lines.
- **Say each fact once,** on the thing that owns it. Never name callers in a comment; that goes stale.

## Codebase specifics

- **Keep both deck modes in step.** Edit mode (`components/Deck.svelte`) and mix mode (`components/mix/MixDeck.svelte`) are built from the shared parts in `components/deck/`. When you change a deck control, change the shared part rather than adding a mode-specific copy. Put mode differences in props, such as `playOnly` or `showBeatLoop`.

- **Use the shared helpers.** Get shortcut hints from `useShortcut()` or `shortcutHint()`, not from hard-coded key names. Use the theme tokens in `app.css` (such as `bg-wave`) rather than literal colours.

## Checks

Run these from `app/` before finishing:

```sh
npx svelte-check --tsconfig ./tsconfig.web.json
npx eslint <changed files>
npm test
```

Only run Prettier (`npx prettier --plugin prettier-plugin-svelte --write <files>`) on the files you changed. Some files elsewhere don't match the formatter yet, and reformatting them adds unrelated noise to the diff.
