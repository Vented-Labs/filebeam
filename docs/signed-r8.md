# Android Test-Signed R8 Smoke

This handoff is for an already-built unsigned R8 APK. It does not rebuild Rust,
run instrumentation, establish release provenance, or use a production key.

## One-time local test key

Keep these files under ignored `.filebeam/` only:

```sh
mkdir -p .filebeam
keytool -genkeypair -keystore .filebeam/android-test.p12 -storetype PKCS12 \
  -storepass android-test -keypass android-test -alias filebeam-test \
  -dname 'CN=Filebeam test signing' -validity 2 -keyalg RSA -keysize 2048
printf '%s\n' \
  'storeFile=android-test.p12' \
  'storePassword=android-test' \
  'keyAlias=filebeam-test' \
  'keyPassword=android-test' > .filebeam/android-test-signing.properties
```

`testSigningPropertiesFile` can override that properties path. All four keys are
required: `storeFile`, `storePassword`, `keyAlias`, and `keyPassword`.

## Post-build smoke

After the network/build owner has produced
`mobile/android/app/build/outputs/apk/release/app-release-unsigned.apk`, run:

```sh
bash scripts/android/run.sh ./gradlew :app:signedR8Smoke
```

The task first runs `check-native.py` on the unsigned R8 APK, including its ABI
alignment checks. It then signs a separate
`app-release-test-signed.apk` with the local key and runs `apksigner verify`.
It does not replace the unsigned APK and does not claim production signing.

## Runtime follow-up

The current device harness installs debug APKs only and is owned by the Android
network agent. Do not change it for this smoke. Once that owner provides a
release-target instrumentation or launch path, use the test-signed APK with the
existing `NativeCoreTest` crypto and WebRTC self-tests, and record the emulator,
APK digest, and test-key identity as test evidence only. App Links validation is
separate: it requires explicit domain association inputs when enabled and is not
satisfied by this test key.
