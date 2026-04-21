# 実機テスト手順

## 事前準備（PC側）
1. Windows 10 / 11 に iTunes または Apple Devices アプリがインストール済みか確認
2. `AppleMobileDeviceService` が Windows サービスとして起動していることを確認
3. `cargo build` が成功することを確認

## テスト手順

### Step 1: 起動
```bash
cd C:\project\test\ios-remote
RUST_LOG=ios_remote=debug cargo run
```

起動ログに以下が出ることを確認：
- `API token (Bearer): xxxxx` — メモしておく
- `Local-only mode — use --lan to expose on all interfaces.` または LAN 警告
- `Web dashboard: http://127.0.0.1:8080`

### Step 2: iPhone接続
1. USB Type-Cケーブルで接続
2. iPhone側で「このコンピュータを信頼しますか？」→「信頼」をタップ

### Step 3: ログ観察
ターミナルに表示されるログを確認:
- `Connected to usbmuxd at 127.0.0.1:27015` → usbmuxd接続OK
- `Connected to iPhone` → lockdownd接続OK
- `Starting screen capture via USB...` → 画面キャプチャ開始

### Step 4: エラー時の対処
| ログメッセージ | 原因 | 対処 |
|---|---|---|
| `Cannot connect to usbmuxd` | iTunes/Apple Devices が未インストール、またはサービス停止 | インストール＆サービス起動 |
| `No iPhone connected` | USB未接続 or 信頼未承認 | ケーブル確認 + 信頼タップ |
| `Still waiting for iPhone` が繰り返し | 30 秒タイムアウト後の定期警告（正常動作） | そのまま待機 or ケーブル差し直し |
| `Requested UDID ... not connected` | `--device <UDID>` の指定ミス | `--list-devices` で正しい UDID 確認 |
| `Multiple devices connected — using first` | 2 台以上接続 | `--device` でターゲット固定 |

### Step 5: Web Dashboard 動作確認
1. ブラウザで `http://127.0.0.1:8080` を開く
2. 「Connected: (device-name)」が表示されることを確認
3. Screenshot ボタン → `recordings/` 配下に PNG が保存される
4. Start Recording → Stop Recording → `recordings/rec_*.h264` が作成される

### Step 6: 認証 / LAN 確認
1. `curl http://127.0.0.1:8080/api/stats` → `401 Unauthorized` を確認（トークンなし）
2. `curl -H "Authorization: Bearer <TOKEN>" http://127.0.0.1:8080/api/stats` → `200` で JSON 取得
3. `--lan` 付きで再起動し、同 LAN 上の別 PC からブラウザで http://<IP>:8080 にアクセス（トークン必須）
4. `--lan` なしで別 PC からアクセス → 接続拒否されることを確認

### Step 7: 再接続耐性
1. 起動中に USB ケーブルを抜く → ログに `Device disconnected` と再試行が出る
2. 抜いたまま 30 秒待つ → `Still waiting for iPhone` 警告が出る
3. 再挿入 → 自動で復帰する

### Step 8: 複数デバイス
1. 2 台目の iPhone を接続
2. `--list-devices` を別コマンドで実行 → 両方の UDID が表示
3. `--device <UDID>` で片方を固定起動

### Step 9: マクロ（WDA 使用時）
1. iPhone に署名済み WebDriverAgent をインストール
2. `iproxy 8100 8100` 等で USB ポートを転送
3. `macros/test.json` に Tap アクションを書く
4. `POST /api/macros/run` で送信 → iPhone 上でタップが反映される
5. WDA を停止した状態で実行 → エラーレスポンスが返り、プロセスは落ちない

### Step 10: 結果報告
テスト後、以下をメモして共有:
1. どこまで進んだか（usbmuxd接続/デバイス検出/lockdown/screenshotr/Web/再接続/マクロ）
2. エラーメッセージ（あれば全文）
3. iPhoneのモデルとiOSバージョン
4. Windows のバージョンと Apple Mobile Device Service のバージョン
