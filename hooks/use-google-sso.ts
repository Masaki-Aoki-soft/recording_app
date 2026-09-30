'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { useSignIn, useSignUp } from '@clerk/react/legacy';
import toast from 'react-hot-toast';

import { clerkErrorMessage } from '@/lib/clerk-errors';
import { cancelSso, errorMessage, getSsoRedirectUrl, isTauri, waitForSsoCallback } from '@/lib/tauri';

/**
 * Clerk の Google ソーシャルログイン。
 *
 * Google は WebView 内の OAuth をブロックするため、認証はシステムブラウザで行い、
 * Clerk が Rust のループバックサーバーへ返す rotating_token_nonce でセッションを確定する
 * （@clerk/expo の useSSO と同じフロー）。未登録のユーザーはそのままサインアップに切り替える。
 */
export function useGoogleSso() {
    const { isLoaded: signInLoaded, signIn, setActive } = useSignIn();
    const { isLoaded: signUpLoaded, signUp } = useSignUp();
    const router = useRouter();
    const [pending, setPending] = useState(false);

    const start = async () => {
        if (!signInLoaded || !signUpLoaded || pending) return;
        if (!isTauri()) {
            toast.error('Google ログインはデスクトップアプリでのみ利用できます');
            return;
        }

        setPending(true);
        try {
            const redirectUrl = await getSsoRedirectUrl();
            const attempt = await signIn.create({ strategy: 'oauth_google', redirectUrl });
            const authUrl = attempt.firstFactorVerification.externalVerificationRedirectURL;
            if (!authUrl) throw new Error('Google の認証 URL を取得できませんでした');

            const callbackUrl = new URL(await waitForSsoCallback(authUrl.toString()));
            const nonce = callbackUrl.searchParams.get('rotating_token_nonce');
            if (!nonce) {
                // 原因の切り分け用に、Clerk が返したパラメータ名（値は秘匿情報を含み得るので出さない）を記録
                const keys = [...callbackUrl.searchParams.keys()];
                console.error('SSO callback without rotating_token_nonce. params:', keys);
                const clerkError =
                    callbackUrl.searchParams.get('__clerk_status') ?? callbackUrl.searchParams.get('error');
                throw new Error(
                    `ログイン結果を受け取れませんでした（Clerk ダッシュボードの「Allowlist for mobile SSO redirect」に ${redirectUrl} が登録されているか確認してください）` +
                        (clerkError ? ` [${clerkError}]` : '') +
                        (keys.length ? ` 受信パラメータ: ${keys.join(', ')}` : ' 受信パラメータ: なし'),
                );
            }

            const reloaded = await attempt.reload({ rotatingTokenNonce: nonce });

            let sessionId = reloaded.createdSessionId;
            if (reloaded.firstFactorVerification.status === 'transferable') {
                // Google アカウントに対応するユーザーがいない → 新規登録に切り替える
                const created = await signUp.create({ transfer: true });
                sessionId = created.createdSessionId;
                if (created.status !== 'complete') {
                    throw new Error('アカウント作成に追加の情報が必要です。Clerk の必須項目の設定を確認してください');
                }
            } else if (reloaded.status !== 'complete') {
                throw new Error('追加の認証が必要なアカウントです。メールアドレスとパスワードでログインしてください');
            }

            if (!sessionId) throw new Error('セッションを作成できませんでした');
            await setActive({ session: sessionId });
            toast.success('Google アカウントでログインしました');
            router.replace('/dashboard');
        } catch (err) {
            const message = typeof err === 'string' ? errorMessage(err) : clerkErrorMessage(err, 'Google ログインに失敗しました');
            toast.error(message);
        } finally {
            setPending(false);
        }
    };

    const cancel = () => {
        cancelSso().catch(console.error);
    };

    return { start, cancel, pending, ready: signInLoaded && signUpLoaded };
}
