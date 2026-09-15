# Website UX Parity

## Changes

- The download-with-CLI card now offers commands for live WebRTC file links and
  pending Turbo file links with an accepted encrypted early descriptor.
- The CLI install help now identifies HTTP, progressive Turbo, and live WebRTC
  file sends and receives.
- The card continues to withhold commands for notes, burn-on-read transfers, and
  account inbox delivery. Those APIs are not available to the CLI.
- File-send actions are explicitly labelled **Send encrypted** and **Turbo
  Transfer**, with visible `Enter` and `Shift+Enter` shortcuts.
- In file mode, those shortcuts work only when focus is not in an editable or
  actionable control. Note editors, note titles, inputs, buttons, and other
  controls retain their native Enter behavior, and repeated keydown events do
  not start a second transfer.
- CLI links remain single-quoted literally. A separate key and a password are
  never appended to the command.

## Verification

- `npm run check`: passed.
- `BASE_URL=http://127.0.0.1:8019 npx playwright test
  tests/browser/cli-commands.spec.ts tests/browser/cli.spec.ts
  tests/browser/turbo.spec.ts`: 20 passed against the disposable backend.
