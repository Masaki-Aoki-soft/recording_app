/**
 * Tauri（WebView2）向けに Clerk を「ネイティブモード」で動かすための Clerk インスタンス生成。
 *
 * - ブラウザの Cookie ではなく、FAPI の Authorization ヘッダでクライアントを識別する（Expo SDK と同じ方式）。
 *   Tauri の origin（http://tauri.localhost）でもセッションが維持でき、
 *   システムブラウザでの SSO 完了後に rotating_token_nonce でセッションを受け取れる。
 * - clerk-js を CDN から読み込まず、バンドルした @clerk/clerk-js を使う。
 */
import type { Clerk as ClerkType } from '@clerk/clerk-js';

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

export async function createNativeClerk(publishableKey: string): Promise<ClerkType> {
    if (instance) return instance;

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
