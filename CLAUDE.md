# Working in this repo

## Code philosophy

- **DRY (Don't Repeat Yourself).** Eliminate duplication by creating reusable, consistent components and shared logic. Maintain a single source of truth so changes are made once, reducing complexity, errors and maintenance overhead.

- **Self-Documenting Code (strict).** Write code that clearly communicates its intent through meaningful names, simple structures and expressive methods. Code should explain what it does and why without relying on excessive comments or external documentation.

- **Clarity, Consistency and Maintainability.** Prioritize readable, reusable and intuitive code over clever or overly complex solutions. Make it easy for others (and your future self) to understand, extend and maintain the code with confidence.

- **Leave the codebase better than you found it.** If working in a branch or PR, tidy up code surrounding your change. Don't just stick in the feature and force it to work in the current environment, consider the architecture and how your changes effect it. Assess if the design or maintainability of the codebase is improved or worsened by your change, and vice versa see if your feature would benefit from a refactor of the surrounding code. If you see a better way to do something, at least consider it.

## Codebase specifics
- **Apply comment reduction as you go (moderate to aggressive).**  In areas you touch, try and remove comments that do not explicitly add value. If a comment is needed, consider if the code can be rewritten to make the comment unnecessary. The codebase has a lot of comments right now that are not helpful, and we want to reduce them over time. If you are unsure if a comment is needed, ask a reviewer.

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
