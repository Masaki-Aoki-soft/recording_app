# MeetingRec

スケジュールに従って **Zoom デスクトップアプリで会議に自動参加** し、**会議ウィンドウとシステム音声・マイクを自動録画** して、ローカル保存 + Google Drive へ自動アップロードする Windows アプリです。

- UI: Next.js（static export）+ shadcn/ui
- ロジック: Tauri 2 + Rust
- 認証: Clerk（メールアドレス + パスワード）
- 録画: Windows Graphics Capture（映像）+ WASAPI ループバック（音声）→ FFmpeg で mp4 化

## 仕組み

```
scheduler ──(開始 1 分前)──▶ session
                              ├─ zoommtg:// で Zoom アプリを起動して参加（ブラウザを経由しない）
                              ├─ Zoom の会議ウィンドウが現れるまで待機（待機室・ホスト未開始に対応）
                              ├─ 録画: 会議ウィンドウを WGC でキャプチャ → ffmpeg (libx264)
                              │        システム音声 / マイクを WASAPI で WAV に記録
                              ├─ 停止: 会議終了（ウィンドウ消滅）/ 指定時間 / 手動
                              ├─ ffmpeg で映像と音声を同期して mp4 に mux
                              └─ Google Drive へレジューマブルアップロード（失敗時は自動再試行）
```

録画ファイルは `ビデオ\MeetingRec\` に保存されます。アプリは ✕ で閉じてもタスクトレイに常駐し、スケジュールを実行します。

## セットアップ

### 1. 必要なもの

- [Bun](https://bun.sh/) / Node.js
- Rust（stable）と [Tauri 2 の前提条件](https://v2.tauri.app/start/prerequisites/)（WebView2, MSVC Build Tools）
- Zoom デスクトップアプリ

### 2. FFmpeg（sidecar）

FFmpeg は 100MB 超のため git 管理していません。libx264 を含むビルド（例: [gyan.dev の essentials build](https://www.gyan.dev/ffmpeg/builds/)）の `ffmpeg.exe` を次の名前で配置してください。

```
src-tauri/bin/ffmpeg-x86_64-pc-windows-msvc.exe
```

### 3. 環境変数

| ファイル | キー | 内容 |
| --- | --- | --- |
| `.env.local` | `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` | Clerk ダッシュボードの Publishable key |
| `src-tauri/.env` | `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` | Google Cloud の OAuth クライアント（種類: **デスクトップアプリ**） |

テンプレートは `.env.example` と `src-tauri/.env.example` にあります。Google の値はビルド時にバイナリへ埋め込まれます（未設定でもビルドは通り、Drive 連携時にエラーになります）。

**Clerk の設定**（ダッシュボード）:

1. **User & authentication**: 「Email address」+「Password」を有効化
2. **SSO connections**: 「Google」を追加（本番インスタンスでは Google Cloud で作成した OAuth クライアントを設定）
3. **Native applications**: 「Enable Native API」をオン
4. **Native applications → Allowlist for mobile SSO redirect** に `http://127.0.0.1:47615/sso-callback` を追加

アプリは Clerk を Expo SDK と同じ「ネイティブモード」（Cookie ではなく Authorization ヘッダでセッション管理）で動かします。Google は WebView 内の OAuth をブロックするため、「Google でログイン」はシステムブラウザで認証し、Clerk が上記のループバック URL に返す `rotating_token_nonce` でアプリ側のセッションを確定させます。ポート 47615 が他のアプリに使われているとログインできません。

**Google Cloud の設定**: Drive API を有効化し、OAuth 同意画面のスコープに `drive.file` と `userinfo.email` を追加してください。

### 4. 起動 / ビルド

```bash
bun install
bun tauri dev      # 開発
bun tauri build    # NSIS インストーラを作成
```

### Windows の Smart App Control について

Smart App Control（スマート アプリ コントロール）が有効な PC では、Rust のビルドスクリプトや proc-macro（署名されていない DLL/EXE）の実行がブロックされ、`cargo build` が「アプリケーション制御ポリシーによってこのファイルがブロックされました (os error 4551)」で失敗します。その場合は Smart App Control を無効にした PC、または GitHub Actions（`.github/workflows/ci.yml`）でビルドしてください。

## Zoom 側の推奨設定

プロトコル URL ではオーディオ/ビデオを制御できないため、Zoom アプリの設定で次を有効にしてください。

- 「ミーティングへの参加時に、コンピューターでオーディオに参加」
- 「ミーティングに参加する際、マイクをミュートに設定」「ビデオをオフにする」
- 「ビデオプレビューダイアログを表示」をオフ

## 注意

会議を録画する際は、主催者・参加者の同意を得てください。本アプリは画面キャプチャで録画するため、Zoom の録画通知は他の参加者に表示されません。

## テスト

```bash
bun run lint
cargo test --manifest-path src-tauri/Cargo.toml
```
