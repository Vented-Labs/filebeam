# FilebeamTheme

Platform-independent colour and syntax logic for the iOS app: the palette engine that
reproduces the web theme generator (`backend/app/Support/Theming`) and the note
highlighter. Colours are plain `0xRRGGBB` integers so the package has no UI dependency
and its tests run on macOS with `swift test`.
