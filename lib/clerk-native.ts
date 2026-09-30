/**
 * Tauri（WebView2）向けに Clerk を「ネイティブモード」で動かすための Clerk インスタンス生成。
 *
 * - ブラウザの Cookie ではなく、FAPI の Authorization ヘッダでクライアントを識別する（Expo SDK と同じ方式）。
 *   Tauri の origin（http://tauri.localhost）でもセッションが維持でき、
 *   システムブラウザでの SSO 完了後に rotating_token_nonce でセッションを受け取れる。
 * - FAPI は Origin と Authorization の両方を持つリクエストを拒否するが、WebView の fetch は Origin を
 *   必ず付けるため、Clerk へのリクエストは Rust（clerk_fetch コマンド）経由で送る。
 * - clerk-js を CDN から読み込まず、バンドルした @clerk/clerk-js を使う。
 */
import { invoke } from '@tauri-apps/api/core';
import type { Clerk as ClerkType } from '@clerk/clerk-js';

import { isTauri } from '@/lib/tauri';

/** クライアントトークンの保存キー（WebView の localStorage に保存） */
const CLIENT_JWT_KEY = 'meetingrec.clerk.client_jwt';

let instance: ClerkType | null = null;

function readToken(): string | null {
    try {
        return window.localStorage.getItem(CLIENT_JWT_KEY);
    } catch {
        return null;
    }
}

function writeToken(token: string) {
    try {
        window.localStorage.setItem(CLIENT_JWT_KEY, token);
    } catch {
        // 保存できない場合は次回起動時に再ログインになるだけ
    }
}

/** Publishable key（pk_test_xxx / pk_live_xxx）から Frontend API のホスト名を取り出す */
function frontendApiHost(publishableKey: string): string {
    const encoded = publishableKey.split('_')[2] ?? '';
    return atob(encoded).replace(/\$$/, '');
}

interface ProxyResponse {
    status: number;
    headers: [string, string][];
    body: string;
}

/** Clerk FAPI へのリクエストだけを Rust 経由（Origin ヘッダ無し）で送るように fetch を差し替える */
function installFapiFetchProxy(host: string) {
    const originalFetch = window.fetch.bind(window);

    window.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
        const request = new Request(input, init);
        if (new URL(request.url).host !== host) {
            return originalFetch(input, init);
        }

        const hasBody = request.method !== 'GET' && request.method !== 'HEAD';
        const result = await invoke<ProxyResponse>('clerk_fetch', {
            request: {
                method: request.method,
                url: request.url,
                headers: [...request.headers.entries()],
                body: hasBody ? await request.text() : null,
            },
        });

        // 204 / 304 などは本文を持てない
        const nullBody = [101, 204, 205, 304].includes(result.status);
        return new Response(nullBody ? null : result.body, {
            status: result.status,
            headers: result.headers,
        });
    };
}

export async function createNativeClerk(publishableKey: string): Promise<ClerkType> {
    if (instance) return instance;

    // clerk-js を読み込む前に差し替える（Tauri 外のブラウザプレビューでは通常の fetch のまま）
    if (isTauri()) {
        installFapiFetchProxy(frontendApiHost(publishableKey));
    }

    const { Clerk } = await import('@clerk/clerk-js');
    const clerk = new Clerk(publishableKey);

    clerk.__internal_onBeforeRequest(async (requestInit) => {
        requestInit.credentials = 'omit';
        requestInit.url?.searchParams.append('_is_native', '1');
        const headers = requestInit.headers as Headers | undefined;
        headers?.set('authorization', readToken() ?? '');
    });

    clerk.__internal_onAfterResponse(async (_request, response) => {
        const authorization = response?.headers.get('authorization');
        if (authorization) writeToken(authorization);
    });

    instance = clerk;
    return clerk;
}
