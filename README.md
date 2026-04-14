# ios-remote

<p align="center">
  <strong>USB Type-C経由でiPhoneの画面をPCにミラーリング＆操作するツール</strong><br>
  Pure Rust製 / Windows対応 / MIT License
</p>

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
```

### 必要なもの

| 項目 | 詳細 |
|------|------|
| **USB Type-Cケーブル** | Lightning-to-Cでも可 |
| **iTunes / Apple Devices** | Windowsにインストール（usbmuxdドライバ提供） |
| **「信頼」承認** | iPhone側で「このコンピュータを信頼しますか？」→「信頼」 |
| **Rust 1.80+** | ビルド用 |

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
- **アプリ使用時間** — 画面から自動でアプリ滞在時間を集計
- **画面変化ハイライト** — フレーム間差分を色付き表示
- **セッションリプレイ** — 全操作を記録し後で再現

### Developer Tools
- **コマンドパレット** — 35コマンドをファジー検索
- **プロトコルアナライザー** — RTSP/usbmuxdメッセージの詳細ログ
- **ネットワーク診断** — ping / 遅延 / ジッター測定
- **帯域スロットリング** — ネットワーク使用量を制限
- **接続タイムライン** — イベントの時系列ビジュアル
- **ベンチマーク** — デコード/レンダリング性能測定

### System Integration
- **Web Dashboard** — ブラウザでステータス確認＆操作（http://localhost:8080）
- **REST API** — 12エンドポイントでプログラマブル制御
- **設定ファイル** — TOML形式の永続設定
- **接続履歴** — 過去に接続したデバイスを記録
- **Windows起動時自動起動** — レジストリ登録
- **システムトレイ** — 最小化して常駐
- **自動アップデーター** — GitHub Releasesから新版を通知
- **ポータブルモード** — USBメモリから実行可能
- **i18n** — 日本語 / English / 中文 / 한국어
- **テーマ** — Dark / Light / Midnight / Nature

## Web Dashboard

ブラウザで `http://localhost:8080` を開くとリアルタイムダッシュボードが使えます。

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
| ffmpeg | RTMP配信 / 録画変換 | [ffmpeg.org](https://ffmpeg.org) |
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
├── features/            All feature modules (68)
│   ├── display.rs        Window rendering
│   ├── recording.rs      Video recording
│   ├── screenshot.rs     PNG capture
│   ├── ocr.rs            Text extraction
│   ├── ai_vision.rs      Claude API vision
│   ├── ...               (60+ more modules)
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

# Run tests
cargo test
```

## CI/CD

GitHub Actions で Windows / Linux / macOS の自動ビルドとリリースを実行:

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
