# FilebeamDomain

Foundation-only Swift 6 domain contract for the iOS UI and UniFFI bridge. It contains no networking, SwiftUI, platform storage, or fixture implementation.

## Contract

All public domain models are `Sendable`; persisted presentation facts are also `Codable`. `FilebeamInstance(origin:)` accepts only a canonical HTTPS origin. It deliberately rejects paths, fragments, queries, and credentials.

`FilebeamService` is the one bridge surface. Its workflow groups are:

- Discovery and files: `discover`, `inspectReceive`, `startUpload`, `receive`, `startInboxReceive`, `resumeInboxReceive`.
- Jobs: `snapshot(jobID:)`, `pause(jobID:)`, `resume(transferID:)`, `respondToPrompt(jobID:promptID:value:)`, `respondToConsent(jobID:promptID:allowed:)`, `discardLocal`, `revokeRemote`, and `endLive`.
- Notes: `startNote`, `inspectNote`, `startReceiveNote`, `takeVerifiedNote(jobID:)`, and `retryNoteBurn(jobID:)`.
- Account and inbox: login/register/session/logout, reset/verification, recipient lookup, inbox listing/unread/mark/delete, inbox metadata/open, and preferences.
- Keys: list/situation, generate, password wrap/unwrap, validate/upload, self-custody export/import.
- Receipts and backend workflows: `receipt`, `inspectInvite`, `acceptInvite`, `report`, and `requestAccountDeletion`.

`NativeFilebeamService` implements the protocol in the bridge target, mapping native policy responses to `InstancePolicy`, active jobs to `TransferSnapshot`, and saved details to `TransferRecord`. Snapshots carry a stable `TransferJobID`, transfer/checkpoint IDs, prompts, verified results, server availability, and explicit `ExportStatus`. A share URL can exist while an upload is running, and a verified download can remain private until exported.

Secret-bearing request types are `Sendable`, not `Codable`: passwords, key material, cookies, reset tokens, and report contact addresses must remain caller-owned. `AccountKeyBundle.encryptedPrivateKey` is persisted remote historical key metadata required for password custody, not plaintext key material.

Use `InputRouter.route(_:selectedInstance:)` before inspection. For a transfer URL, pass `InputRoute.transfer.input` unchanged to FFI: it retains the full fragment/key. The route's instance is the link instance, while a bare ID uses the selected instance. `ExternalRequestCoordinator` queues entries rather than replacing an active draft or consent prompt.

Validate a copy shown in an options sheet with `SendPolicy.validate`. Applying a validated copy is a UI decision; the functions never mutate either draft. A `.unknown` result means size/planning is unavailable, not success.

`FileOptionsTransaction` and `NoteOptionsTransaction` represent independent temporary sheet edits. `commit()` returns the edited options; `cancel()` returns the original options. Neither changes a `FileDraft` or `NoteDraft` until the UI explicitly assigns the result.
