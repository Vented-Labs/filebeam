#!/usr/bin/env bash
set -euo pipefail
: "${RUNNER_TEMP:?}" "${GITHUB_ENV:?}"
umask 077
case ${RUNNER_OS:?} in
    macOS)
        keychain="$RUNNER_TEMP/filebeam.keychain-db"
        password=$(openssl rand -base64 32)
        printf '::add-mask::%s\n' "$password"
        printf '%s' "${APPLE_CERTIFICATE_BASE64:?}" | base64 -D > "$RUNNER_TEMP/filebeam.p12"
        printf '%s' "${APPLE_NOTARY_KEY_BASE64:?}" | base64 -D > "$RUNNER_TEMP/filebeam-notary.p8"
        security create-keychain -p "$password" "$keychain"
        security set-keychain-settings -lut 21600 "$keychain"
        security unlock-keychain -p "$password" "$keychain"
        security import "$RUNNER_TEMP/filebeam.p12" -k "$keychain" -P "${APPLE_CERTIFICATE_PASSWORD:?}" -T /usr/bin/codesign
        security list-keychains -d user -s "$keychain" login.keychain-db
        security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain"
        xcrun notarytool store-credentials filebeam-notary --keychain "$keychain" --key "$RUNNER_TEMP/filebeam-notary.p8" --key-id "${APPLE_NOTARY_KEY_ID:?}" --issuer "${APPLE_NOTARY_ISSUER:?}"
        printf 'APPLE_NOTARY_PROFILE=filebeam-notary\n' >> "$GITHUB_ENV"
        ;;
    Windows)
        printf '%s' "${WINDOWS_CERTIFICATE_BASE64:?}" | base64 --decode > "$RUNNER_TEMP/filebeam.pfx"
        printf 'WINDOWS_SIGNING_CERTIFICATE=%s\n' "$RUNNER_TEMP/filebeam.pfx" >> "$GITHUB_ENV"
        powershell.exe -NoProfile -Command '$sdk = Get-ChildItem "${env:ProgramFiles(x86)}/Windows Kits/10/bin/*/x64/signtool.exe" | Sort-Object FullName -Descending | Select-Object -First 1; if (!$sdk) { throw "signtool not installed" }; $sdk.DirectoryName | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append'
        ;;
    *) exit 64 ;;
esac
