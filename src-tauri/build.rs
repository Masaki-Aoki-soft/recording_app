use std::collections::HashSet;

/// .env ファイルを順に読み、`accept` に一致するキーを rustc-env としてバイナリへ埋め込む。
/// 優先順位: 環境変数 > 先に指定したファイル
fn embed_env(files: &[&str], accept: impl Fn(&str) -> Option<&'static str>) {
    let mut embedded = HashSet::new();
    for file in files {
        println!("cargo:rerun-if-changed={}", file);
        let Ok(iter) = dotenvy::from_filename_iter(file) else {
            continue;
        };
        for (key, value) in iter.flatten() {
            let Some(target) = accept(&key) else {
                continue;
            };
            if std::env::var(&key).is_ok() || !embedded.insert(target) {
                continue;
            }
            println!("cargo:rustc-env={}={}", target, value);
        }
    }
}

fn main() {
    // Google OAuth のクライアント情報（src-tauri/.env.local / .env）。未設定でもビルドは通り、Drive 連携時にエラーになる
    println!("cargo:rerun-if-env-changed=GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=GOOGLE_CLIENT_SECRET");
    embed_env(&[".env.local", ".env"], |key| match key {
        "GOOGLE_CLIENT_ID" => Some("GOOGLE_CLIENT_ID"),
        "GOOGLE_CLIENT_SECRET" => Some("GOOGLE_CLIENT_SECRET"),
        _ => None,
    });

    // Clerk の Publishable key（フロントと同じルートの .env.local / .env）。Clerk 通信プロキシの送信先の制限に使う
    println!("cargo:rerun-if-env-changed=NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY");
    if let Ok(key) = std::env::var("NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY") {
        println!("cargo:rustc-env=CLERK_PUBLISHABLE_KEY={}", key);
    } else {
        embed_env(&["../.env.local", "../.env"], |key| {
            (key == "NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY").then_some("CLERK_PUBLISHABLE_KEY")
        });
    }

    tauri_build::build()
}
