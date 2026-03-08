#!/bin/bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_BIN="$HOME/Downloads/WumaTracker.app/Contents/MacOS/wuma-tracker"

if [[ ! -x "$APP_BIN" ]]; then
  echo "앱 실행 파일을 찾을 수 없습니다:"
  echo "$APP_BIN"
  echo ""
  echo "~/Download 폴더에 WumaTracker.app이 있는지 확인하세요."
  read -r -p "엔터를 누르면 종료합니다..."
  exit 1
fi

echo "관리자 권한으로 WumaTracker를 실행합니다."
echo "비밀번호를 입력하면 앱이 실행됩니다."
sudo "$APP_BIN"
