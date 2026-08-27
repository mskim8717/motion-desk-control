#!/bin/bash
# MotionDesk.app 번들 + .dmg 생성 (Apple Silicon 전용, 서명/공증 없음)
# 사용: scripts/bundle.sh → target/release/bundle/osx/MotionDesk.dmg
set -euo pipefail
cd "$(dirname "$0")/.."

# --format osx: .app만 생성 (기본값이면 plist 패치 전의 dmg까지 만들어 버림)
cargo bundle --release -p desk-tray --format osx
rm -rf target/release/bundle/dmg

APP="target/release/bundle/osx/MotionDesk.app"
PLIST="$APP/Contents/Info.plist"

# cargo-bundle이 지원하지 않는 Info.plist 항목 주입
/usr/libexec/PlistBuddy -c "Delete :NSBluetoothAlwaysUsageDescription" "$PLIST" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :NSBluetoothAlwaysUsageDescription string 책상과 통신하기 위해 Bluetooth를 사용합니다." "$PLIST"
/usr/libexec/PlistBuddy -c "Delete :LSUIElement" "$PLIST" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :LSUIElement bool true" "$PLIST"

# Info.plist를 수정했으므로 ad-hoc 재서명 (Apple Silicon은 서명 없는 arm64 실행 불가)
codesign --force --deep -s - "$APP"

# .dmg 생성: 앱 + Applications 심볼릭 링크가 든 스테이징 폴더를 이미지로 변환
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

DMG="target/release/bundle/osx/MotionDesk.dmg"
rm -f "$DMG"
hdiutil create -volname "MotionDesk" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null

echo "완료: $DMG"
