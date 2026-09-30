'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { useSignIn } from '@clerk/react/legacy';
import toast from 'react-hot-toast';

import { clerkErrorMessage } from '@/lib/clerk-errors';
import { cancelSso, errorMessage, isTauri } from '@/lib/tauri';

/**
 * Clerk の Google ソーシャルログイン。
 *
 * Google は WebView 内の OAuth をブロックするため、ClerkProvider に登録した OAuth transport
 * （lib/clerk-native.ts）経由でシステムブラウザで認証し、deep link（meetingrec://sso-callback）で
 * アプリに戻す。コールバック後の reload・セッション確定・未登録ユーザーのサインアップ移行は clerk-js が行う。
 */
export function useGoogleSso() {
    const { isLoaded, signIn } = useSignIn();
    const router = useRouter();
    const [pending, setPending] = useState(false);

    const start = async () => {
        if (!isLoaded || pending) return;
        if (!isTauri()) {
            toast.error('Google ログインはデスクトップアプリでのみ利用できます');
            return;
        }

        setPending(true);
        let completed = false;
        // セッション確定以外の遷移（追加の本人確認が必要な場合など）の行き先
        let unsupportedStep: string | null = null;

        try {
            // redirectUrl / redirectUrlComplete は transport の deep link で上書きされる
            await signIn.authenticateWithRedirect({
                strategy: 'oauth_google',
                redirectUrl: '/dashboard',
                redirectUrlComplete: '/dashboard',
                __internal_callbackParams: {
                    signInUrl: '/login',
                    signUpUrl: '/sign-up',
                    __internal_navigateOnSetActive: async () => {
                        completed = true;
                    },
                    __internal_navigate: async (to: string) => {
                        unsupportedStep = to;
                    },
                },
            } as Parameters<typeof signIn.authenticateWithRedirect>[0]);

            if (completed) {
                toast.success('Google アカウントでログインしました');
                router.replace('/dashboard');
            } else if (unsupportedStep) {
                console.warn('SSO requires an additional step:', unsupportedStep);
                toast.error('このアカウントは追加の認証が必要です。メールアドレスとパスワードでログインしてください');
            } else {
                toast.error('ログインを完了できませんでした。もう一度お試しください');
            }
        } catch (err) {
            const message =
                typeof err === 'string' ? errorMessage(err) : clerkErrorMessage(err, 'Google ログインに失敗しました');
            toast.error(message);
        } finally {
            setPending(false);
        }
    };

    const cancel = () => {
        cancelSso().catch(console.error);
    };

    return { start, cancel, pending, ready: isLoaded };
}
