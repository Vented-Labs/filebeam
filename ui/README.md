# Filebeam UI

`@filebeam/ui` is Filebeam's private workspace package for shared Vue transfer, note, account, and layout components. It is source code for this repository, not a separately published package.

Build from the repository root:

```sh
npm ci
npm run build
```

The root build generates the encryption WebAssembly package before building the backend assets. Backend workspace commands alone do not rebuild it.

## Clipboard input

On the Send screen, paste text to open Notes or paste an image to add it to Files.
Existing drafts are preserved. Paste inside an editor or form field behaves
normally; the Paste action on the file dropzone appends text to the note draft.

On mobile web, touch and hold the dropzone and choose **Paste**. The same action
is available as a button and through the keyboard context menu. If the browser
cannot read the clipboard directly, a paste field opens for its native Paste
command. Image availability depends on the browser and clipboard source; use
**Choose files** when the browser cannot expose the image. Sharing still requires
the Send action.
