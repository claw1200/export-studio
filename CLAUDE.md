# Working in this repo

## Leave the code better than you found it

Within the area you are working on, tidy as you go. This applies to the files and components your change touches, not unrelated parts of the codebase.

- **Don't repeat yourself.** Before writing something, look for an existing component, helper or getter that already does it, and reuse it. If you write the same logic or class string a second time, extract it: a shared component, a snippet, a helper in `lib/`, or a getter on the model (for example, `player.playable`).
- **Make the code explain itself.** Prefer clear names for variables, props and functions over comments that say what the code does. Remove comments that only restate the code.
- **Keep the comments that explain why.** Keep a comment where the code can't say it: a non-obvious reason, a constraint, or a workaround.
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
