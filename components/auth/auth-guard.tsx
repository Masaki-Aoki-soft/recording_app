'use client';

import { useEffect } from 'react';
import { useRouter } from 'next/navigation';
import { useAuth } from '@clerk/react';
import { Loader2 } from 'lucide-react';

/**
 * サインイン済みのときだけ children を表示する。
 * `requireSignedOut` を指定するとログイン画面用に逆の判定（サインイン済みなら /dashboard へ）。
 */
export default function AuthGuard({
    children,
    requireSignedOut = false,
}: {
    children: React.ReactNode;
    requireSignedOut?: boolean;
}) {
    const { isLoaded, isSignedIn } = useAuth();
    const router = useRouter();

    const allowed = isLoaded && (requireSignedOut ? !isSignedIn : !!isSignedIn);

    useEffect(() => {
        if (!isLoaded || allowed) return;
        router.replace(requireSignedOut ? '/dashboard' : '/login');
    }, [isLoaded, allowed, requireSignedOut, router]);

    if (!allowed) {
        return (
            <div className="min-h-screen flex items-center justify-center bg-zinc-50 dark:bg-zinc-950">
                <Loader2 className="h-6 w-6 animate-spin text-zinc-400" />
            </div>
        );
    }

    return <>{children}</>;
}
