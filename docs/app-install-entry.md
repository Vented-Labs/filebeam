# App installation entry

The Vue header uses `AppInstallButton.vue`. Set the build-time Vite variable
`VITE_APP_INSTALL_URL` to the real, platform-neutral app installation page when it
is published. Rebuild the frontend after configuring it. The component also accepts
a `destination` prop for an application-owned destination.

That page owns OS support, capability checks, and installed-state handling. The
header's Desktop/Mobile labels follow the shell layout breakpoint (desktop above
900 CSS pixels), not OS detection. The destination opens in a new tab with
`noopener noreferrer` so an in-progress transfer and editor remain mounted.

Until a valid destination is supplied, the header entry is focusable but marked
`aria-disabled`, with the explanation “App installation is not available yet.” It
does not open a preview dialog or the CLI installer. No app package or store URL
is currently configured in this checkout; publishing that destination is an
integration dependency for the desktop/mobile implementation.

The desktop footer opens the existing CLI dialog. Contextual Install CLI actions
remain available at all widths. If the footer is hidden during an open dialog,
focus returns to the visible header app entry on dismissal.

Review the actual shell at `http://127.0.0.1:4178/?placement` after running
`npm run dev:prism`; the CLI gallery also includes the production launcher and app
entry for pointer, pressed, keyboard-focus, and open/return inspection.
