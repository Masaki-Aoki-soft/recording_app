fn main() {
    // Google OAuth のクライアント情報を src-tauri/.env.local / .env（または環境変数）から読み込み、
    // コンパイル時にバイナリへ埋め込む。未設定でもビルドは通り、Drive 連携時にエラーになる。
    // 優先順位: 環境変数 > .env.local > .env
    println!("cargo:rerun-if-env-changed=GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=GOOGLE_CLIENT_SECRET");

    let mut embedded = std::collections::HashSet::new();
    for file in [".env.local", ".env"] {
        println!("cargo:rerun-if-changed={}", file);
        let Ok(iter) = dotenvy::from_filename_iter(file) else {
            continue;
        };
        for (key, value) in iter.flatten() {
            if key.starts_with("GOOGLE_")
                && std::env::var(&key).is_err()
                && embedded.insert(key.clone())
            {
                println!("cargo:rustc-env={}={}", key, value);
            }
        }
    }

    tauri_build::build()
}
