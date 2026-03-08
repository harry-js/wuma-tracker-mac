# Wuma Tracker (명조 맵스 트래커)

Wuma Tracker는 게임 **명조: 워더링 웨이브**의 인게임 플레이어 위치를 실시간으로 공유하고 웹 맵과 연동할 수 있도록 도와주는 데스크톱 애플리케이션입니다. 이 프로그램을 사용하면 친구와 함께 탐험할 때 서로의 위치를 쉽게 확인하거나, 개인 방송 등에서 자신의 위치를 지도 위에 표시하는 등 다양하게 활용할 수 있습니다.

Tauri, SvelteKit, TypeScript를 기반으로 제작되었습니다.

-----

## 🌟 주요 기능

  * **실시간 위치 추적**: 게임 클라이언트의 메모리를 직접 읽어 플레이어의 좌표(x, y, z)와 방향(pitch, yaw, roll) 데이터를 실시간으로 감지합니다.
  * **WebRTC 기반 위치 공유**: WebRTC 기술을 활용하여 다른 유저와 P2P로 위치 데이터를 공유하거나, 로컬 웹소켓 서버를 통해 다른 애플리케이션으로 데이터를 전송할 수 있습니다.
  * **간편한 연결 코드**: 8자리의 Base36 코드를 생성하여 다른 사용자와 쉽게 연결을 공유할 수 있습니다.
  * **외부 서버 연동**: 중앙 서버(`wss://concourse.wuwa.moe`)와 연결하여 원격으로 위치 데이터를 공유하는 기능을 지원합니다.
  * **자동 업데이트**: 새로운 버전이 출시되면 앱 내에서 자동으로 업데이트를 확인하고 설치할 수 있습니다.
  * **직관적인 UI**: SvelteKit과 Tailwind CSS를 기반으로 한 깔끔하고 사용하기 쉬운 인터페이스를 제공합니다.

-----

## 📖 사용 방법

1.  **프로그램 실행**: Wuma Tracker를 실행합니다.
2.  **게임 프로세스 연결**: `프로세스 찾기 및 연결` 버튼을 클릭하여 실행 중인 '명조: 워더링 웨이브' 클라이언트에 연결합니다.
3.  **위치 공유**:
      * **친구와 공유**: `외부 연결 시작` 버튼을 눌러 생성된 8자리 코드를 친구에게 알려주세요. 친구는 해당 코드를 사용하여 당신의 위치를 지도에서 볼 수 있습니다.
      * **외부 프로그램 연동**: 로컬 웹소켓 주소(`ws://127.0.0.1:46821`)를 사용하여 OBS 같은 방송 프로그램이나 다른 웹 애플리케이션에 실시간 위치 데이터를 전송할 수 있습니다.

-----

## 💻 개발 환경 설정

이 프로젝트를 개발하기 위한 추천 환경입니다.

  * **[VS Code](https://code.visualstudio.com/)**: 코드 에디터
  * **[Svelte for VS Code](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode)**: Svelte 언어 지원
  * **[Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)**: Tauri 프레임워크 지원
  * **[rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)**: Rust 언어 지원

-----

## 🚀 시작하기

1.  **저장소 복제**:

    ```bash
    git clone https://github.com/wuwamoe/wuma-tracker.git
    cd wuma-tracker
    ```

2.  **의존성 설치**:
    이 프로젝트는 `pnpm`을 사용합니다.

    ```bash
    pnpm install
    ```

3.  **개발 서버 실행**:
    Tauri 개발 환경을 시작합니다.

    ```bash
    pnpm tauri dev
    ```

### macOS 참고사항

- macOS에서도 빌드/실행은 가능합니다.
- 다만 게임 프로세스 메모리 접근은 `task_for_pid` 권한이 필요하므로, 개발 환경에서는 관리자 권한으로 실행해야 연결이 가능합니다.
- 게임 프로세스 이름은 `Client-Mac-Shipping`, `Wuthering Waves`, `WutheringWaves` 순서로 자동 탐색합니다.

### macOS 빌드 방법

로컬 사용 기준(업데이트 서명 생략)으로 `.app`만 빌드:

```bash
node -e 'const fs=require("fs"); const p="src-tauri/tauri.conf.json"; const o=JSON.parse(fs.readFileSync(p,"utf8")); o.bundle=o.bundle||{}; o.bundle.createUpdaterArtifacts=false; fs.writeFileSync("src-tauri/tauri.conf.localbuild.json", JSON.stringify(o,null,2));'
pnpm tauri build --bundles app --config src-tauri/tauri.conf.localbuild.json
codesign --force --deep --sign - src-tauri/target/release/bundle/macos/WumaTracker.app
xattr -dr com.apple.quarantine src-tauri/target/release/bundle/macos/WumaTracker.app
rm -f src-tauri/tauri.conf.localbuild.json
```

빌드 결과:
- `src-tauri/target/release/bundle/macos/WumaTracker.app`

### macOS 빌드 후 실행 방법

1. Finder에서 아래 앱을 직접 실행합니다.
   - `src-tauri/target/release/bundle/macos/WumaTracker.app`
2. 실행 차단이 뜨면 우클릭 -> `열기`로 1회 허용합니다.
3. 게임 연결 시 권한 이슈가 있으면 아래 중 하나를 사용합니다.

권장(일반 사용자용):
- `시스템 설정 -> 개인정보 보호 및 보안 -> 개발자 도구(Developer Tools)`에서
  사용 중인 터미널(iTerm/Terminal) 또는 앱을 허용한 뒤 다시 실행

확실한 방법(개발/테스트):
```bash
sudo src-tauri/target/release/bundle/macos/WumaTracker.app/Contents/MacOS/wuma-tracker
```

-----

## 📂 프로젝트 구조

```
.
├── src/                      # SvelteKit 프론트엔드 소스 코드
│   ├── lib/                  # Svelte 라이브러리 (컴포넌트, 유틸리티 등)
│   ├── routes/               # SvelteKit 라우팅
│   └── app.html              # 메인 HTML 템플릿
├── src-tauri/                # Rust 백엔드 (Tauri) 소스 코드
│   ├── src/
│   │   ├── main.rs           # 프로그램 진입점
│   │   ├── lib.rs            # Tauri 앱 설정 및 명령어
│   │   ├── rtc_supervisor.rs # WebRTC 및 데이터 수집 총괄
│   │   ├── peer_manager.rs   # WebRTC Peer 연결 관리
│   │   ├── signaling_handler.rs # WebSocket 시그널링 처리
│   │   ├── native_collector.rs # 게임 프로세스 데이터 수집
│   │   └── win_proc.rs       # Windows 프로세스 메모리 접근
│   ├── Cargo.toml            # Rust 의존성 관리
│   └── tauri.conf.json       # Tauri 설정 파일
├── package.json              # Node.js 의존성 및 스크립트
└── svelte.config.js          # SvelteKit 설정 파일
```
