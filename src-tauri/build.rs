fn main() {
    // Google OAuth のクライアント情報を src-tauri/.env（または環境変数）から読み込み、
    // コンパイル時にバイナリへ埋め込む。未設定でもビルドは通り、Drive 連携時にエラーになる。
    println!("cargo:rerun-if-changed=.env");
    println!("cargo:rerun-if-env-changed=GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=GOOGLE_CLIENT_SECRET");
    if let Ok(iter) = dotenvy::from_filename_iter(".env") {
        for (key, value) in iter.flatten() {
            if key.starts_with("GOOGLE_") && std::env::var(&key).is_err() {
                println!("cargo:rustc-env={}={}", key, value);
            }
        }
    }

    tauri_build::build()
}
