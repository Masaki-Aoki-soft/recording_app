/* ルートページ: サインイン状態に応じて振り分け */

'use client';

import { useEffect } from 'react';
import { useRouter } from 'next/navigation';
import { useAuth } from '@clerk/react';
import { Loader2 } from 'lucide-react';

export default function RootPage() {
    const { isLoaded, isSignedIn } = useAuth();
    const router = useRouter();

    useEffect(() => {
        if (!isLoaded) return;
        router.replace(isSignedIn ? '/dashboard' : '/login');
    }, [isLoaded, isSignedIn, router]);

    return (
        <div className="min-h-screen flex items-center justify-center bg-zinc-50 dark:bg-zinc-950">
            <Loader2 className="h-6 w-6 animate-spin text-zinc-400" />
        </div>
    );
}
