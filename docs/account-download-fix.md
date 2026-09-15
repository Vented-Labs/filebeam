# Account Download Fix

Native inbox downloads use authenticated native inbox metadata and chunk routes.
The recipient working key and cookie are request-scoped, never written to a
checkpoint or converted into a public link. Inbox checkpoints require the host
to restore the recorded origin's account session and supply the key again.

The native logout request now explicitly requests JSON, matching the existing
session controller's representation negotiation.
