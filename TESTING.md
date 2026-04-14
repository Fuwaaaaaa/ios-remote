# 実機テスト手順

## 事前準備（PC側）
1. iTunes または Apple Devices アプリがインストール済みか確認
2. `cargo build` が成功することを確認

## テスト手順

### Step 1: 起動
```bash
cd C:\project\test_branch\ios_remort
RUST_LOG=ios_remote=debug cargo run
```

### Step 2: iPhone接続
1. USB Type-Cケーブルで接続
2. iPhone側で「このコンピュータを信頼しますか？」→「信頼」をタップ

### Step 3: ログ観察
ターミナルに表示されるログを確認:
- `Connected to usbmuxd` → usbmuxd接続OK
- `Device found!` → iPhone検出OK
- `Lockdownd connected` → lockdownd接続OK
- `Screenshotr connected` → 画面キャプチャ開始

### Step 4: エラー時の対処
| ログメッセージ | 原因 | 対処 |
|---|---|---|
| `Cannot connect to usbmuxd` | iTunesが未インストール | iTunesをインストール |
| `No iPhone connected` | USB未接続 or 信頼未承認 | ケーブル確認 + 信頼タップ |
| `StartService failed` | ペアリング不完全 | iPhoneのロックを解除して再接続 |
| `Screenshot receive failed` | プロトコル不一致 | ログの詳細を共有してください |

### Step 5: 結果報告
テスト後、以下をメモして共有:
1. どこまで進んだか（usbmuxd接続/デバイス検出/lockdown/screenshotr）
2. エラーメッセージ（あれば全文）
3. iPhoneのモデルとiOSバージョン
