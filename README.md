# ios-remote

<p align="center">
  <strong>USB Type-C経由でiPhoneの画面をPCにミラーリング＆操作するツール</strong><br>
  Pure Rust製 / <strong>Windows 10 / 11 専用</strong> / MIT License
</p>

> ⚠️ **プラットフォーム:** 本ツールは Windows 10 / 11 **専用** です。macOS / Linux ではビルド時に `build.rs` がエラーを出します。これは `AppleMobileDeviceService` (Windows 版 iTunes / Apple Devices 付属) に依存しているためです。

---

## Overview

iPhoneをUSBケーブルでPCに接続するだけで、iPhoneの画面をリアルタイムでPCに表示します。Wi-Fi不要。脱獄不要。

```
iPhone ──USB-C──> PC (ios-remote) ──> ディスプレイウィンドウ
                                  ──> 録画 / スクリーンショット
                                  ──> Web Dashboard
```

## Quick Start

```bash
# ビルド
cargo build

# 起動（iPhoneをUSBで接続してから）
cargo run

# PiPモード（常に最前面）
cargo run -- --pip

# 録画付き
cargo run -- --record

# 接続中のデバイス一覧（UDID 確認用）
cargo run -- --list-devices

# 特定の iPhone を UDID 指定で使う
cargo run -- --device 00008120-001A2B3C4D5E6F78

# LAN の他 PC からダッシュボードを使う（Bearer トークン必須）
cargo run -- --lan
```

### 初回起動時のログ

起動直後に以下のようなログが出ます。`API token` の値をメモしてください（設定ファイルにも保存されます）。

```
INFO  API token (Bearer): kQ3m7dF2-sLaP9xR0cT8vB1n
INFO  Local-only mode — use --lan to expose on all interfaces.
INFO  Web dashboard: http://127.0.0.1:8080
```

- 既定は `127.0.0.1` にのみバインドするため、同じ PC の Web ブラウザからのみ到達可能です。
- `--lan` を付けると `0.0.0.0` にバインドし、LAN の他ホストからもアクセスできます。その際 API トークンは必須です。
- トークンは `IOS_REMOTE_API_TOKEN` 環境変数で上書き、または `ios-remote.toml` の `[network] api_token` で固定できます。

### 必要なもの

| 項目 | 詳細 |
|------|------|
| **Windows 10 / 11** | 本ツールの動作 OS（他 OS 非対応） |
| **USB Type-Cケーブル** | Lightning-to-Cでも可 |
| **iTunes / Apple Devices** | Windowsにインストール（AppleMobileDeviceService / usbmuxd ドライバ提供） |
| **「信頼」承認** | iPhone側で「このコンピュータを信頼しますか？」→「信頼」 |
| **Rust 1.80+** | ビルド用 |

### 対応状況

- ✅ **Windows 10 / 11** — ネイティブ対応
- ❌ **macOS / Linux** — 未対応（`build.rs` が build を拒否します）
- ❌ **AirPlay モード** — v0.4.0 で削除、USB Type-C 一本化

## Features

### Core — USB画面ミラーリング
- **USB Type-C直結** — Wi-Fi不要、低遅延
- **usbmuxd通信** — Appleの公式USBプロトコルを使用
- **lockdowndセッション** — デバイス情報取得＆サービス起動
- **screenshotrキャプチャ** — iPhoneの画面をリアルタイム取得

### Display
- **PiPモード** — 常に最前面の小窓表示（`--pip`）
- **アスペクト比維持** — レターボックスで画面崩れなし
- **統計オーバーレイ** — FPS / 遅延 / 解像度をリアルタイム表示
- **タッチオーバーレイ** — タップ位置にリップルアニメーション
- **ホットキー** — `S`=スクリーンショット, `Q`/`Esc`=終了

### Recording & Capture
- **動画録画** — H.264ストリームをファイル保存（`--record`）
- **スクリーンショット** — PNG保存（ホットキー or API）
- **GIF生成** — 直近N秒をアニメーションGIFで保存
- **タイムラプス** — 定期フレームキャプチャ
- **モーション録画** — 画面変化時のみ録画（容量節約）

### Screen Analysis
- **OCR** — tesseract連携でテキスト抽出（日本語+英語）
- **AI画面理解** — Claude APIで「今何が映っているか」を解析
- **通知キャプチャ** — iOSバナー通知を自動検出＆保存
- **QRコード検出** — 画面上のQR/バーコードを自動スキャン
- **カラーピッカー** — マウス位置の色をHEX/RGB/HSLで取得

### Automation
- **マクロ** — JSON形式でタップ/スワイプ/待機を記録・再生
- **Luaスクリプト** — Lua 5.4で高度な自動化（`--features lua`）
- **ジェスチャーライブラリ** — ピンチ/回転/3指スワイプ等のプリセット
- **音声コマンド** — 「スクリーンショット」と話して撮影
- **スケジュールタスク** — cron式で定期実行

### Visual Tools
- **アノテーション** — 画面上に矢印/四角/テキスト/フリーハンドを描画
- **ルーラー** — ピクセル距離計測
- **デバイスフレーム** — iPhone筐体フレームをオーバーレイ（SS/動画用）
- **iOSセーフエリア表示** — ノッチ/ダイナミックアイランドの安全領域
- **デザイングリッド** — 8pt/4ptグリッドオーバーレイ
- **色覚シミュレーション** — 色覚異常の見え方を再現
- **プライバシーモード** — 特定領域をぼかし/ピクセレート
- **ウォーターマーク** — 録画/配信に透かし

### Streaming & Sharing
- **RTMP配信** — Twitch/YouTubeへffmpeg経由でライブ配信
- **OBS仮想カメラ** — 名前付きパイプでOBSに映像入力
- **MJPEG共有** — ブラウザで画面をリモート閲覧
- **Imgur即共有** — ワンキーでSS→アップロード→URL取得
- **通知転送** — Discord / Slack / Telegram に検出通知を送信

### Analytics
- **タッチヒートマップ** — クリック頻度のビジュアル化
- **画面変化ハイライト** — フレーム間差分を色付き表示
- **セッションリプレイ** — 記録セッションを Web Dashboard の Replay カードから再生 (`/api/replay/*`)。デコードに ffmpeg を利用 (下の Optional Dependencies 参照)

### Developer Tools
- **コマンドパレット** — 35コマンドをファジー検索
- **プロトコルアナライザー** — RTSP/usbmuxdメッセージの詳細ログ
- **ネットワーク診断** — ping / 遅延 / ジッター測定
- **帯域スロットリング** — ネットワーク使用量を制限
- **接続タイムライン** — イベントの時系列ビジュアル

### System Integration
- **Web Dashboard** — ブラウザでステータス確認＆操作（http://localhost:8080）
- **REST API** — 16エンドポイントでプログラマブル制御
- **設定ファイル** — TOML形式の永続設定
- **接続履歴** — 過去に接続したデバイスを記録
- **Windows起動時自動起動** — レジストリ登録
- **システムトレイ** — 最小化して常駐
- **自動アップデーター** — GitHub Releasesから新版を通知
- **ポータブルモード** — USBメモリから実行可能
- **i18n** — 日本語 / English / 中文 / 한국어
- **テーマ** — Dark / Light / Midnight / Nature

## Troubleshooting

| 症状 | 確認ポイント |
|------|--------------|
| `Cannot connect to usbmuxd` | iTunes / Apple Devices が起動し `AppleMobileDeviceService` が Windows サービスとして走っているか |
| `No iPhone connected` | USB ケーブル、接続ポート、iPhone 側の「信頼」タップ |
| 画面が固まる | USB-C ケーブルがデータ通信対応か（充電専用ケーブルでは動きません） |
| 起動直後に自動再接続を繰り返す | `--list-devices` で UDID を確認し、`--device <UDID>` で固定 |
| ブラウザで Web Dashboard に `401 Unauthorized` | 起動ログの API token を確認、URL 直打ちではなく `/` から開くかヘッダ付きで叩く |
| `Failed to bind Web dashboard` | `-w <PORT>` で別ポートを指定 |
| 複数 iPhone を同時接続したい | 現時点では 1 台ずつ。`--device` で切り替え |

## Macro setup (iOS 入力送信)

`MacroAction::Tap` / `Swipe` / `LongPress` は [WebDriverAgent (WDA)](https://github.com/appium/WebDriverAgent) を介して iPhone に入力を送ります。`screenshotr` サービスは読み取り専用なので、入力には Apple Developer 証明書で署名した WDA のサイドロードが必須です。

1. WDA を Xcode でビルドし iPhone にインストール
2. iPhone 上で WDA を一度起動し、ポート 8100 で待ち受けていることを確認
3. PC 側で `iproxy 8100 8100` 等で USB ポートを転送
4. 起動時に環境変数 `IOS_REMOTE_WDA_URL=http://127.0.0.1:8100` を指定（既定でも同じ値）
5. `POST /api/macros/run` か `F7` キーでマクロ実行

WDA が起動していない場合、`Tap`/`Swipe`/`LongPress` アクションはエラーで返りますが、プロセスは落ちません（`Wait` や `Screenshot` アクションは引き続き動きます）。

## Session Replay

`recordings/` に保存されたセッション (`session.json` / `bookmarks.json` / `video.h264` を含むディレクトリ) を Web Dashboard の Replay カードから再生できます。

### 手順

1. `F2` または `POST /api/recording/start` でセッションを記録 → 停止で `recordings/session_YYYYMMDD_HHMMSS/` が作成されます
2. Dashboard を開き、Replay カードの **Refresh** で一覧を更新
3. セッションを選択して **Load** → ヘッダ情報 (解像度 / フレーム数 / 長さ) とブックマークが表示されます
4. **Play** で再生開始、**Pause** で停止
5. スライダーまたはブックマークボタンでシーク (再生中のシークは拒否されるので、一度 Pause してからシークしてください)

### ffmpeg 依存

デコードは ffmpeg サブプロセス (`-f h264 -i pipe:0 -f rawvideo -pix_fmt rgba pipe:1`) で行います。未インストールの場合 `POST /api/replay/play` は `{ "status": "error", "error": "spawn ffmpeg: ..." }` を返します。下の Optional Dependencies 参照。

### 既知の制限

- シーク位置は比例マッピング (NAL 単位、タイムスタンプ精度は粗い)
- 再生速度は 1.0× 固定
- ffmpeg 未インストール時は録画 / Replay / RTMP がすべて no-op になります (エンコーダも ffmpeg を使うため)

## Web Dashboard

ブラウザで `http://localhost:8080` を開くとリアルタイムダッシュボードが使えます。トークンはダッシュボード HTML にインラインで埋め込まれ、fetch 呼び出しに自動付与されます。カード構成: Status / Actions / Replay / Log / Connection History。

### REST API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/status` | GET | 接続状態 |
| `/api/stats` | GET | ストリーム統計 |
| `/api/screenshot` | POST | スクリーンショット撮影 |
| `/api/recording/start` | POST | 録画開始 |
| `/api/recording/stop` | POST | 録画停止 |
| `/api/ocr` | POST | テキスト抽出 |
| `/api/ai/describe` | POST | AI画面解析 |
| `/api/config` | GET/POST | 設定読み書き |
| `/api/history` | GET | 接続履歴 |
| `/api/macros` | GET | マクロ一覧 |
| `/api/macros/run` | POST | マクロ実行 |
| `/api/replay/sessions` | GET | 記録セッション一覧 |
| `/api/replay/load` | POST | セッションをロード |
| `/api/replay/play` | POST | 再生開始 |
| `/api/replay/pause` | POST | 一時停止 |
| `/api/replay/seek` | POST | シーク (pause 中のみ) |

## Configuration

`ios-remote.toml` で設定をカスタマイズ:

```toml
[receiver]
name = "ios-remote"

[display]
pip_mode = false
show_stats = true
show_touch_overlay = true

[recording]
auto_record = false
output_dir = "recordings"

[network]
bind_address = "127.0.0.1"   # "0.0.0.0" にすると LAN 公開。--lan でも同等
lan_access = false            # true にすると bind_address を強制的に 0.0.0.0 扱いに
api_token = ""                # 空で起動すると自動生成してここに書き込まれます

[features]
notification_capture = true
ocr = false
ai_vision = false
```

## Architecture

```
┌─────────┐     USB Type-C      ┌──────────────┐
│ iPhone  │ ──────────────────>  │  usbmuxd     │
└─────────┘                      │ (port 27015) │
                                 └──────┬───────┘
                                        │
                                 ┌──────▼───────┐
                                 │  lockdownd   │
                                 │ (port 62078) │
                                 └──────┬───────┘
                                        │
                                 ┌──────▼───────┐
                                 │ screenshotr  │
                                 │ (PNG capture)│
                                 └──────┬───────┘
                                        │
                                 ┌──────▼───────┐
                                 │   FrameBus   │──> Display Window
                                 │ (broadcast)  │──> Recording
                                 │              │──> Screenshot / GIF
                                 │              │──> OCR / AI / QR
                                 │              │──> OBS / RTMP / MJPEG
                                 │              │──> Heatmap / Analysis
                                 └──────────────┘
                                        │
                                 ┌──────▼───────┐
                                 │ Web Dashboard│
                                 │  :8080       │
                                 └──────────────┘
```

## Hotkeys

| Key | Action |
|-----|--------|
| `S` | スクリーンショット |
| `Q` / `Esc` | 終了 |
| `P` | PiP切替 |
| `R` | ズームリセット |
| `F2` | 録画開始/停止 |
| `F3` | OCR実行 |
| `F4` | 統計オーバーレイ切替 |
| `F5` | ゲームモード切替 |
| `G` | GIF保存 |
| `I` | カラーピッカー |
| `M` | ルーラー |
| `Scroll` | ズーム |

## Optional Dependencies

| ツール | 用途 | インストール |
|--------|------|-------------|
| tesseract-ocr | OCRテキスト抽出 | [tesseract](https://github.com/tesseract-ocr/tesseract) |
| ffmpeg | RTMP配信 / 録画変換 / セッションリプレイのデコード | [ffmpeg.org](https://ffmpeg.org) |
| `ANTHROPIC_API_KEY` | AI画面理解 | [anthropic.com](https://console.anthropic.com) |
| `OPENAI_API_KEY` | 音声文字起こし | [openai.com](https://platform.openai.com) |
| `IMGUR_CLIENT_ID` | Imgur即共有 | [imgur.com/account/settings/apps](https://imgur.com/account/settings/apps) |

## Project Structure

```
src/
├── main.rs              Entry point + CLI
├── config.rs            TOML settings + connection history
├── error.rs             Error types
├── usb/                 USB connection (core)
│   ├── usbmuxd.rs       usbmuxd protocol client
│   ├── lockdown.rs       lockdownd client
│   ├── screen_capture.rs screenshotr capture loop
│   └── device.rs         device management
├── features/            All feature modules (65 default + 6 experimental)
│   ├── display.rs        Window rendering
│   ├── recording.rs      Video recording
│   ├── screenshot.rs     PNG capture
│   ├── ocr.rs            Text extraction
│   ├── ai_vision.rs      Claude API vision
│   ├── ...               (55+ more modules; 6 gated behind `experimental`)
│   └── zoom.rs           Zoom & pan
├── ui/                  Web interface
│   ├── api.rs            REST API (axum)
│   └── web.rs            Browser dashboard
├── system/              OS integration
│   ├── tray.rs           System tray
│   ├── startup.rs        Auto-start
│   ├── updater.rs        Update checker
│   ├── portable.rs       Portable mode
│   └── installer.rs      NSIS script generator
├── devtools/            Developer tools
│   ├── command_palette.rs Command search
│   ├── protocol_analyzer.rs Protocol logger
│   ├── network_diag.rs   Network diagnostics
│   ├── timeline.rs       Event timeline
│   └── throttle.rs       Bandwidth control
└── idevice/             USB device integration (stubs)
    ├── device_info.rs     Device info
    ├── file_transfer.rs   File transfer (AFC)
    └── syslog.rs          System log relay
```

## Build

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# With Lua scripting
cargo build --features lua

# Stream Deck ボタン連携（HID、要実機）
cargo build --features stream_deck

# ローカル Whisper 文字起こし（要 ggml モデル）
cargo build --features whisper

# 実験機能 (app_detector / benchmark / mouse_gesture / pdf_export / presentation / video_filter)
cargo build --features experimental

# Run tests
cargo test
```

## CI/CD

GitHub Actions で **Windows の自動ビルドとリリース**を実行します（macOS / Linux はビルド対象外）:

```bash
# タグを打ってリリース
git tag v0.4.0
git push --tags
```

## Contributing

1. Fork this repository
2. Create a feature branch
3. `cargo test` が通ることを確認
4. Pull Request を作成

## License

MIT License — see [LICENSE](LICENSE) for details.

---

<p align="center">
  Made with Rust 🦀
</p>
