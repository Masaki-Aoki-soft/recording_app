'use client';

import { useEffect } from 'react';
import { ClerkProvider, useAuth } from '@clerk/react';
import { Toaster } from 'react-hot-toast';

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
    if (!publishableKey) {
        return (
            <div className="min-h-screen flex items-center justify-center p-6 text-center text-sm text-red-600">
                NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY が設定されていません。.env.local を確認してください。
            </div>
        );
    }

    return (
        <ClerkProvider publishableKey={publishableKey} afterSignOutUrl="/login">
            <AuthStateSync />
            {children}
            <Toaster position="bottom-right" />
        </ClerkProvider>
    );
}
