# Device-to-device transfers

Send files and folders directly between your own computers. Both computers sign
in to the same account on the same instance and run the Filebeam daemon. File
names, contents, notes, and receipts travel over an encrypted WebRTC connection
between the two devices. The instance registers devices and relays connection
setup only; it can see which devices connect, when, and from which addresses.

Linux and macOS are supported through the Beam CLI. The desktop app on Linux can
manage the same daemon.

## Requirements

- An administrator enables **Registered-device rendezvous** under **Admin >
  Instance settings**, or sets `FILEBEAM_DEVICE_RENDEZVOUS_ENABLED=true`.
  WebRTC transfers must also be enabled.
- The instance is served over HTTPS.
- Each computer runs `filebeamd` for its Filebeam profile.

## Set up a device

```sh
beam account login you@example.com
beam service enable
beam devices add --name Laptop
```

`beam service enable` installs the signed daemon component and starts it as a
per-user service (a systemd user unit on Linux, a LaunchAgent on macOS). In the
desktop app, use **Settings > Install device service**.

The first device on an account is active immediately. Each later device prints
its fingerprint and an approval payload. On a device that is already active,
compare the fingerprint shown on both screens, then approve it:

```sh
beam devices approve --fingerprint FINGERPRINT   # paste the payload when prompted
```

Approval also pairs the approving device with the new one. Rerun
`beam devices add` on the new device within the enrollment window it printed
(10 minutes) to finish.

Devices send only to devices they have paired with, and accept only from them.
On the new device, pair with each device you will use:

```sh
beam devices list
beam devices pair Desktop --fingerprint FINGERPRINT
```

## Receive

Receiving is off until you enable it, and files go only to destinations you
create:

```sh
beam destinations add dropzone ~/Filebeam/Dropzone --auto-approve
beam receive enable
```

Deliveries are accepted only into destinations created with `--auto-approve`.
Received files are never opened or executed. A name that already exists gets a
numbered suffix instead of being replaced.

## Send

```sh
beam to Laptop report.pdf
beam to Laptop ./photos --into projects
beam jobs list
beam jobs wait JOB_ID
```

`beam to` returns exit code 10 once the encrypted snapshot is prepared; after
that, the original files can be moved or deleted. Add `--wait` to wait for the
receiver's signed receipt. Directories keep their structure, including empty
folders and hidden files. Without `--into`, files go to the receiver's
`dropzone` destination.

If the receiving device is offline, the daemon retries with increasing delays
of up to five minutes. A job that is not delivered within seven days fails.
`beam jobs list` shows the last error for jobs that are waiting or failed. Use
`beam jobs pause`, `resume`, or `cancel` to control a job.

## Revoke a device

Revoke a lost device from **Account > My devices** on the web, or with
`beam devices revoke DEVICE_ID`. Revocation takes effect at the instance
immediately: the device's credential stops working, and other devices stop
transfers with it within a minute of their next registry refresh. A device that
stays offline may not learn about a revocation until it reconnects or its
cached registry is seven days old.

## Troubleshooting

On Linux, the daemon logs to the user journal:

```sh
journalctl --user -u filebeam-daemon
```

`beam receive status` reports whether the daemon is running and receiving.
