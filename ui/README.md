# Filebeam UI

`@filebeam/ui` is Filebeam's private workspace package for shared Vue transfer, note, account, and layout components. It is source code for this repository, not a separately published package.

Build from the repository root:

```sh
npm ci
npm run build
```

The root build generates the encryption WebAssembly package before building the backend assets. Backend workspace commands alone do not rebuild it.
