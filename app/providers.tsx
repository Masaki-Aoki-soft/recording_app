'use client';

import { useEffect, useState } from 'react';
import { ClerkProvider, useAuth } from '@clerk/react';
import type { Clerk } from '@clerk/clerk-js';
import { Loader2 } from 'lucide-react';
import { Toaster } from 'react-hot-toast';

import { createNativeClerk } from '@/lib/clerk-native';
import { isTauri, setSignedIn } from '@/lib/tauri';

const publishableKey = process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY;

/** Clerk のサインイン状態を Rust 側へ同期する（サインイン中のみ自動録画を行うため） */
function AuthStateSync() {
    const { isLoaded, isSignedIn } = useAuth();

    useEffect(() => {
        if (!isLoaded || !isTauri()) return;
        setSignedIn(!!isSignedIn).catch((err) => console.error('Failed to sync auth state:', err));
    }, [isLoaded, isSignedIn]);

    return null;
}

export default function Providers({ children }: { children: React.ReactNode }) {
    const [clerk, setClerk] = useState<Clerk | null>(null);
    const [loadError, setLoadError] = useState<string | null>(null);

    useEffect(() => {
        if (!publishableKey) return;
        createNativeClerk(publishableKey)
            .then(setClerk)
            .catch((err) => {
                console.error('Failed to initialize Clerk:', err);
                setLoadError('認証サービスの初期化に失敗しました');
            });
    }, []);

    if (!publishableKey || loadError) {
        return (
            <div className="min-h-screen flex items-center justify-center p-6 text-center text-sm text-red-600">
                {loadError ?? 'NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY が設定されていません。.env.local を確認してください。'}
            </div>
        );
    }

    if (!clerk) {
        return (
            <div className="min-h-screen flex items-center justify-center bg-zinc-50 dark:bg-zinc-950">
                <Loader2 className="h-6 w-6 animate-spin text-zinc-400" />
            </div>
        );
    }

    return (
        <ClerkProvider
            publishableKey={publishableKey}
            Clerk={clerk}
            // Cookie を使わずヘッダでセッションを管理する（ネイティブモード）
            standardBrowser={false}
            afterSignOutUrl="/login"
        >
            <AuthStateSync />
            {children}
            <Toaster position="bottom-right" />
        </ClerkProvider>
    );
}
