#!/usr/bin/env bash
# Sign (when an identity is available), notarize, and staple a macOS release file.
# Credential names match perfect-sidechain packaging/macos/sign.sh:
#   NOTARYTOOL_PROFILE, or APPLE_ID + APPLE_APP_SPECIFIC_PASSWORD + APPLE_TEAM_ID.
set -euo pipefail

file="${1:-}"
if [[ -z "${file}" || ! -f "${file}" ]]; then
  echo "error: usage: notarize.sh <path-to-dmg-or-pkg>" >&2
  exit 1
fi

if [[ -n "${MACOS_APP_SIGN_IDENTITY:-}" ]]; then
  echo "Signing $(basename "${file}")..."
  sign_args=(--force --timestamp --sign "${MACOS_APP_SIGN_IDENTITY}")
  if [[ -n "${CSC_KEYCHAIN:-}" ]]; then
    sign_args+=(--keychain "${CSC_KEYCHAIN}")
  fi
  codesign "${sign_args[@]}" "${file}"
  codesign --verify --verbose=2 "${file}"
fi

echo "Submitting $(basename "${file}") to Apple notarization (this can take several minutes)..."
if [[ -n "${NOTARYTOOL_PROFILE:-}" ]]; then
  xcrun notarytool submit "${file}" \
    --keychain-profile "${NOTARYTOOL_PROFILE}" \
    --wait
elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_APP_SPECIFIC_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
  xcrun notarytool submit "${file}" \
    --apple-id "${APPLE_ID}" \
    --password "${APPLE_APP_SPECIFIC_PASSWORD}" \
    --team-id "${APPLE_TEAM_ID}" \
    --wait
else
  echo "error: notarization requires NOTARYTOOL_PROFILE or APPLE_ID/APPLE_APP_SPECIFIC_PASSWORD/APPLE_TEAM_ID" >&2
  exit 1
fi

echo "Stapling notarization ticket..."
xcrun stapler staple "${file}"
xcrun stapler validate "${file}"
echo "Notarized and stapled $(basename "${file}")."
